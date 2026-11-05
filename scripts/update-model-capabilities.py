#!/usr/bin/env python3
"""Refresh the offline capability snapshot from an unpacked @earendil-works/pi-ai package.

Usage: python3 scripts/update-model-capabilities.py /path/to/pi-ai --verified-on YYYY-MM-DD
No credentials or model API calls are used. Review the generated diff before release.
"""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("package", type=Path)
parser.add_argument("--verified-on", required=True)
args = parser.parse_args()
package = json.loads((args.package / "package.json").read_text())
assert package["name"] == "@earendil-works/pi-ai"
levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max"]
models = {}


def add(model, provider, model_id=None):
    model_id = model_id or model["id"]
    key = model_id.lower()
    if key in models or model.get("type") != "chat":
        return
    context, output = model.get("contextWindow"), model.get("maxTokens")
    if not all(isinstance(n, int) and n > 0 for n in [context, output]):
        return
    row = {
        "display_name": model["name"],
        "context_window_tokens": context,
        "max_output_tokens": output,
        "input_modalities": model["input"],
        "supports_reasoning": model.get("reasoning", False),
    }
    # Harness's managed route uses Token Market Chat Completions. Never copy a vendor's
    # native protocol, endpoint, auth, or catalog-only compatibility switches.
    compat = {"supportsStore": False, "supportsDeveloperRole": False,
              "maxTokensField": "max_tokens"}
    if row["supports_reasoning"]:
        native = model.get("compat", {})
        fmt = native.get("thinkingFormat", "openai")
        if fmt not in ["openai", "deepseek", "zai", "qwen"]:
            fmt = "openai"
        mapping = model.get("thinkingLevelMap", {})
        efforts = {}
        # Toggle-only models must not advertise several fake effort strengths.
        toggle_only = native.get("supportsReasoningEffort") is False
        if key in ["glm-5", "glm-5.1", "kimi-k2.5", "qwen3.5-plus"]:
            fmt = "zai" if key.startswith("glm-") else "qwen" if key.startswith("qwen") else "deepseek"
            toggle_only = True
        if provider == "anthropic" and native.get("forceAdaptiveThinking"):
            # The gateway maps an explicit thinking flag to adaptive thinking on
            # these models. Always-on Fable routes only need the effort value.
            if not key.startswith("claude-fable-"):
                fmt = "deepseek"
        if key.startswith("minimax-m2") or "deepseek-r1" in key:
            efforts = {"high": "high"}
            fmt, toggle_only = "openai", True
        elif toggle_only:
            if mapping.get("off", "default") is not None:
                efforts["off"] = None if fmt != "openai" else "none"
            efforts["high"] = "high"
        else:
            for level in levels:
                wire = mapping.get(level, level if level not in ["xhigh", "max"] else None)
                if wire is not None:
                    efforts[level] = (None if fmt != "openai" else "none") if level == "off" else wire
        if key.startswith("minimax-m3"):
            # M3 exposes adaptive/disabled, without selectable effort strengths.
            efforts = {"off": "none", "high": "high"}
        gateway_adaptive_claude = key in ["claude-opus-4-7", "claude-opus-4-8", "claude-opus-5", "claude-sonnet-5"]
        if provider == "anthropic" and not (gateway_adaptive_claude or key.startswith("claude-fable-")):
            efforts = None
        if provider == "openrouter":
            # OpenRouter's normalized effort API is not the native vendor API.
            # Do not transplant that gateway's reasoning controls to our route.
            efforts = None
        if efforts is None:
            models[key] = {"source_provider": provider, **row}
            return
        # pi-ai's zai/qwen formats actively disable thinking when no effort was
        # selected. Always-on models must instead use the gateway's canonical
        # effort field: omission preserves the model default; explicit effort
        # is translated by the gateway into the vendor's native dialect.
        if "off" not in efforts and fmt in ["zai", "qwen"]:
            fmt = "openai"
        row["reasoning_efforts"] = efforts
        compat.update(thinkingFormat=fmt, supportsReasoningEffort=not toggle_only)
        if native.get("requiresReasoningContentOnAssistantMessages"):
            compat["requiresReasoningContentOnAssistantMessages"] = True
        row["harness_compat"] = compat
    else:
        row["reasoning_efforts"] = False
    models[key] = {"source_provider": provider, **row}


def rows(provider):
    path = args.package / "dist" / "providers" / "data" / f"{provider}.json"
    return [model for group in json.loads(path.read_text()).values() for model in group.values()]


for provider in ["deepseek", "zai", "moonshotai", "minimax", "openai", "anthropic", "google", "xai", "xiaomi"]:
    for model in rows(provider):
        if any(part in model["id"] for part in ["realtime", "deep-research", "computer-use", "-image", "-live-"]):
            continue
        add(model, provider)
for model in rows("qwen-token-plan-cn"):
    if model["id"].startswith("qwen"):
        add(model, "qwen-token-plan-cn")
# Supplement stable older models absent from the vendors' current catalogs.
for model in rows("opencode"):
    if model["id"].startswith(("deepseek-", "glm-", "kimi-", "minimax-", "qwen")):
        add(model, "opencode")
for model in rows("openrouter"):
    name = model["id"]
    if ":" in name or name.startswith("~"):
        continue
    if name.startswith(("deepseek/", "qwen/", "stepfun/", "bytedance-seed/", "mistralai/")):
        add(model, "openrouter", name.split("/", 1)[1])

# The gateway's public ID differs from the official current DeepSeek alias.
models["deepseek-v4.1-flash"] = dict(models["deepseek-flash"])
snapshot = {
    "schema_version": 1,
    "verified_on": args.verified_on,
    "source": {"package": package["name"], "version": package["version"],
               "repository": "https://github.com/earendil-works/pi/tree/main/packages/ai", "license": "MIT",
               "license_notice": (Path(__file__).resolve().parent / "licenses/pi-ai-MIT.txt").read_text()},
    "models": dict(sorted(models.items())),
}
destination = Path(__file__).resolve().parents[1] / "rust/tokenmarket-core/src/model_capabilities.json"
# Keep each reviewed model on one line, so capacity/effort diffs stay readable.
lines = ["{"]
for key, value in snapshot.items():
    if key != "models":
        lines.append(f"  {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)},")
lines.append('  "models": {')
for index, (key, value) in enumerate(snapshot["models"].items()):
    suffix = "," if index + 1 < len(models) else ""
    lines.append(f"    {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{suffix}")
lines.extend(["  }", "}", ""])
destination.write_text("\n".join(lines))
print(f"Wrote {len(models)} models to {destination}")
