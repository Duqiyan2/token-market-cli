//! Match the SDK's routing contract, independent of app versions and minifier names.
use anyhow::{bail, Result};
use std::collections::HashMap;

pub const ADAPTATION_REQUIRED: &str =
    "当前安装的千问办公尚未适配本地接入方案，请联系管理员适配；原连接和凭据已保留";
pub const DISPATCH: &str = "if(c(),\"direct\"===o.kind)";
const PREFIX: &str = r#"function @dispatcher@(A){return async(e,t,i,n)=>{let r,{route:o,model:s}=e,{accessTimeoutMs:a,onSubmission:l,...g}=i,c=()=>{if(i.signal?.throwIfAborted(),o.identity!==(A?.getModelRequestIdentity()??@identity@("standalone")))throw new @error@("identity_changed")};if(c(),"direct"===o.kind)"#;
const ASSEMBLY: &str = r#"if(t){let e=@provider@(t.provider),i=@type@(e,t.type??r?.type),o=@style@(t.provider,t.style??r?.custom_model_style),s=@url@(t.provider,t.url);n={...e?{provider:e}:{},model:A,...i?{type:i}:{},parameters:{...r?.byok_parameters??{},api_key:t.api_key},...void 0!==s?{url:s}:{},...o?{style:o}:{}}}"#;
const TRANSPORT: &str = r#"let w=@builder@(t,{...g,target:r,beforeSend:@before@,sessionForm:A});c(),Q(),A.assertCustomProviderAccess(o.identity);try{return @stream@(w,n)}finally{l?.()}"#;
const POLICY: &str = r#"function @policy@(A){if(null==A)return;if(!@object@(A))return null;let e=A.model,t=null!=e&&""!==e,i=A.parameters;"#;
const ENTRY: &str = r#"});@entry@(),@cli@(async()=>{let{createCliDependencies:A}=await Promise.resolve().then(()=>(@depsInit@(),@deps@));return A()});"#;
const ENDPOINT: &str = r#"function @resolver@(A,e={}){let{env:t,region:i}=@environment@();"#;
const ENDPOINT_OVERRIDE: &str = r#"let n=@override@();if(n)return{url:n,purpose:A,mode:"server-override",env:t,region:i,reason:"cn_server_endpoint_env"};"#;

pub struct Match<'a> {
    pub start: usize,
    pub end: usize,
    pub bindings: HashMap<&'a str, String>,
}
fn identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

/// Literal templates capture only JS identifiers. Every fixed token must match,
/// and duplicate/ambiguous matches are rejected rather than guessed.
fn matching<'a>(source: &str, template: &'a str) -> Vec<Match<'a>> {
    let parts: Vec<_> = template.split('@').collect();
    let mut matches = vec![];
    for (start, _) in source.match_indices(parts[0]) {
        let mut cursor = start + parts[0].len();
        let mut bindings = HashMap::new();
        let mut valid = true;
        for pair in parts[1..].chunks_exact(2) {
            let end = source.as_bytes()[cursor..]
                .iter()
                .position(|b| !identifier(*b))
                .map(|n| cursor + n)
                .unwrap_or(source.len());
            if cursor == end || source.as_bytes()[cursor].is_ascii_digit() {
                valid = false;
                break;
            }
            let value = &source[cursor..end];
            if bindings.get(pair[0]).is_some_and(|old| old != value)
                || !source[end..].starts_with(pair[1])
            {
                valid = false;
                break;
            }
            bindings.insert(pair[0], value.to_owned());
            cursor = end + pair[1].len();
        }
        if valid {
            matches.push(Match {
                start,
                end: cursor,
                bindings,
            });
        }
    }
    matches
}
pub fn unique<'a>(source: &str, template: &'a str) -> Result<Match<'a>> {
    let mut matches = matching(source, template);
    if matches.len() != 1 {
        bail!(ADAPTATION_REQUIRED)
    }
    Ok(matches.pop().unwrap())
}

pub struct Contract {
    pub dispatcher: String,
    pub assembler: String,
    pub policy: String,
    pub dispatch_init: String,
    pub assembly_init: String,
    pub assembly: String,
    pub legacy_needle: String,
    pub style: String,
    pub url: String,
    pub builder: String,
    pub stream: String,
    pub thinking: String,
    pub endpoint_resolver: String,
    pub endpoint_override: String,
}
impl Contract {
    pub fn recognize(source: &str) -> Result<Self> {
        let dispatch = unique(source, PREFIX)?;
        let dispatch_end = source[dispatch.end..]
            .find("}var ")
            .map(|n| dispatch.end + n)
            .ok_or_else(|| anyhow::anyhow!(ADAPTATION_REQUIRED))?;
        let body = &source[dispatch.start..dispatch_end + 1];
        let transport = unique(body, TRANSPORT)?;
        if source.matches(DISPATCH).count() != 1 || !body.contains("r={kind:\"qoder\"};") {
            bail!(ADAPTATION_REQUIRED)
        }
        let mut init_end = (dispatch_end + 200).min(source.len());
        while !source.is_char_boundary(init_end) {
            init_end -= 1;
        }
        let init = unique(&source[dispatch_end..init_end], "}var @init@=@lazy@(()=>{")?;
        let assembly = unique(source, ASSEMBLY)?;
        let assembly_start = source[..assembly.start]
            .rfind("function ")
            .ok_or_else(|| anyhow::anyhow!(ADAPTATION_REQUIRED))?;
        let signature = unique(
            &source[assembly_start..assembly.start],
            "function @assembler@(A,e,t,i){let n,r=",
        )?;
        let mut tail_end = (assembly.end + 6500).min(source.len());
        while !source.is_char_boundary(tail_end) {
            tail_end -= 1;
        }
        let tail = &source[assembly.end..tail_end];
        if !tail.starts_with("else if(Q&&r)")
            || !tail.contains("functionSwitches:r?.function_switches}}")
        {
            bail!(ADAPTATION_REQUIRED)
        }
        let init_end = tail
            .find("new Set([\"sec_scan\"])")
            .ok_or_else(|| anyhow::anyhow!(ADAPTATION_REQUIRED))?;
        let model_init = unique(&tail[..init_end], "}var @key@,@set@,@init@=@lazy@(()=>{")?;
        // Some SDKs bundle the same protocol parser twice. This binding is
        // used only by the dummy probe; it never alters production routing.
        let policy = matching(source, POLICY)
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!(ADAPTATION_REQUIRED))?;
        let thinking=unique(source,"function @thinking@(){return{modes:[],supportedEffortLevels:[],requiredBetasForEffort:[]}}")?;
        let endpoint = unique(source, ENDPOINT)?;
        let endpoint_end = source[endpoint.end..]
            .find("}function ")
            .map(|n| endpoint.end + n + 1)
            .ok_or_else(|| anyhow::anyhow!(ADAPTATION_REQUIRED))?;
        let endpoint_body = &source[endpoint.start..endpoint_end];
        let endpoint_override = unique(endpoint_body, ENDPOINT_OVERRIDE)?;
        let provider = &assembly.bindings["provider"];
        let style = assembly.bindings["style"].clone();
        let url = assembly.bindings["url"].clone();
        Ok(Self {
            dispatcher: dispatch.bindings["dispatcher"].clone(),
            assembler: signature.bindings["assembler"].clone(),
            policy: policy.bindings["policy"].clone(),
            dispatch_init: init.bindings["init"].clone(),
            assembly_init: model_init.bindings["init"].clone(),
            assembly: format!(
                "o={style}(t.provider,t.style??r?.custom_model_style),s={url}(t.provider,t.url);"
            ),
            legacy_needle: format!("if(t){{let e={provider}(t.provider)"),
            style,
            url,
            builder: transport.bindings["builder"].clone(),
            stream: transport.bindings["stream"].clone(),
            thinking: thinking.bindings["thinking"].clone(),
            endpoint_resolver: endpoint.bindings["resolver"].clone(),
            endpoint_override: endpoint_body[endpoint_override.start..endpoint_override.end].into(),
        })
    }

    pub fn probe_source(&self, source: &str) -> Result<String> {
        let mut tail_start = source.len().saturating_sub(1500);
        while !source.is_char_boundary(tail_start) {
            tail_start -= 1;
        }
        let entry = unique(&source[tail_start..], ENTRY)?;
        if !source[tail_start + entry.end..].trim().is_empty() {
            bail!(ADAPTATION_REQUIRED)
        }
        let mut output = source[..tail_start + entry.start + 3].replace(
            "r={kind:\"qoder\"};",
            "throw new Error(\"Cloud model fallback is forbidden\");r={kind:\"qoder\"};",
        );
        output.push_str(&format!("\n{}();{}();export {{{} as requestDispatcher,{} as parseModelPolicy,{} as assembleModel,{} as resolveEndpoint}};\n",self.dispatch_init,self.assembly_init,self.dispatcher,self.policy,self.assembler,self.endpoint_resolver));
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_contracts_reject_missing_ambiguous_and_inconsistent_identifiers() {
        assert!(unique("afoo(foo)", "a@x@(@x@)").is_ok());
        assert!(unique("afoo(bar)", "a@x@(@x@)").is_err());
        assert!(unique("afoo(foo)afoo(foo)", "a@x@(@x@)").is_err());
        assert!(unique("a1(1)", "a@x@(@x@)").is_err());
    }
}
