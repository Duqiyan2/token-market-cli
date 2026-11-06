// Token Market DuMate unified cold-start adapter v2. No credentials in this file.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import https from 'node:https';
import { Readable } from 'node:stream';
import { createHash } from 'node:crypto';

const SLOT = Symbol.for('cn.tokenmarket.dumate.unified.v3');
const VIRTUAL = 'https://tokenmarket.dumate.invalid/v1';
const FAILURE = 'Token Market 统一接管不可用，请重新接入并重启搭子。';
const MAX_BODY = 32 * 1024 * 1024;
const modelPath = /\/chat\/completions(?:$|[/?])|\/v1(?:beta)?\/(?:responses|messages|embeddings|images\/generations|models\/[^/]+:(?:generateContent|streamGenerateContent))(?:$|[/?])/;
const cloudRun = /ai_assistant\/(?:run|stream|chat|message)(?:$|[/?])/;
const mediaPath = /\/(?:images|videos)(?:\/|$)|\/(?:image|video)\/(?:generations|generate)(?:\/|$)/;
const mediaType = type => /^(?:image|video)(?:\/|_|$)|^input_(?:image|video)$/.test(String(type ?? ''));
const mediaParts = parts => Array.isArray(parts) && parts.some(part =>
  mediaType(part.type) || mediaType(part.mime) || mediaType(part.mimeType)
  || part.type === 'file' && /\.(?:jpe?g|png|gif|webp|bmp|tiff?|svg|mp4|mov|avi|mkv|wmv|flv|webm|m4v|3gp)(?:[?#].*)?$/i.test(part.filename ?? part.url ?? ''));

function snapshot() {
  if (!['darwin', 'win32'].includes(process.platform) || process.env.DUMATE_APP_VER !== '1.0.89.447'
      || ![undefined, '0', '1'].includes(process.env.OPENCODE_NEW_RUNTIME)
      || process.env.DUMATE_LOGIN_MODE === 'enterprise' || process.env['X-Dumate-Prd-Source'] === 'enterprise') {
    throw Error('当前搭子版本或运行模式不支持统一接管。');
  }
  const account = process.env.XDG_CONFIG_HOME;
  if (!account || !path.isAbsolute(account)) throw Error(FAILURE);
  const file = path.join(account, 'desktop-main/dumate-app.json');
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 1024 * 1024) throw Error(FAILURE);
  // A native custom provider captures media too. It must stay disabled.
  if (JSON.parse(fs.readFileSync(file, 'utf8')).customLLMProvider != null) throw Error('原生模型覆盖仍开启，请重新接入并重启搭子。');
  const policyFile = path.join(account, 'opencode/tokenmarket-unified.json');
  const policyStat = fs.lstatSync(policyFile);
  if (!policyStat.isFile() || policyStat.isSymbolicLink() || policyStat.size > 65536) throw Error(FAILURE);
  const policy = JSON.parse(fs.readFileSync(policyFile, 'utf8')).unified;
  const provider = policy?.provider;
  if (!provider || typeof provider.apiKey !== 'string' || !provider.apiKey || typeof provider.model !== 'string' || !provider.model
      || !Number.isSafeInteger(provider.contextLimit) || !Number.isSafeInteger(provider.outputLimit)
      || provider.outputLimit < 1 || provider.contextLimit <= provider.outputLimit) throw Error(FAILURE);
  const base = new URL(provider.baseURL);
  if (base.username || base.password || base.search || base.hash
      || !(base.protocol === 'https:' || base.protocol === 'http:' && ['localhost', '127.0.0.1'].includes(base.hostname))) throw Error(FAILURE);
  const fingerprint = createHash('sha256').update(JSON.stringify(['baseURL', 'apiKey', 'model', 'contextLimit', 'outputLimit'].map(key => provider[key]))).digest('hex');
  if (policy?.protocol !== 3 || policy.owner !== 'cn.tokenmarket' || policy.providerFingerprint !== fingerprint
      || policy.baseURL !== provider.baseURL || policy.model !== provider.model) throw Error('统一接管配置已变化，请重新接入并重启搭子。');
  return Object.freeze({ ...provider, baseURL: base.href.replace(/\/$/, '') });
}
async function bodyText(input, init) {
  const body = init?.body;
  if (typeof body === 'string') {
    if (Buffer.byteLength(body) > MAX_BODY) throw Error('模型请求过大。');
    return body;
  }
  if (body instanceof Uint8Array || body instanceof ArrayBuffer) {
    const bytes = Buffer.from(body); if (bytes.length > MAX_BODY) throw Error('模型请求过大。');
    return bytes.toString('utf8');
  }
  if (input instanceof Request) {
    const reader = input.clone().body?.getReader(); if (!reader) throw Error('模型请求缺少正文。');
    const chunks = []; let length = 0;
    try {
      while (true) {
        const part = await reader.read(); if (part.done) break;
        length += part.value.byteLength; if (length > MAX_BODY) throw Error('模型请求过大。');
        chunks.push(part.value);
      }
    } catch (error) { await reader.cancel().catch(() => {}); throw error; }
    return Buffer.concat(chunks).toString('utf8');
  }
  throw Error('无法识别模型请求，已阻止回退到官方模型。');
}
function forward(provider, payload, signal) {
  const target = new URL(provider.baseURL + '/chat/completions');
  const bytes = Buffer.from(JSON.stringify(payload));
  return new Promise((resolve, reject) => {
    const request = (target.protocol === 'https:' ? https : http).request(target, {
      method: 'POST', agent: false, headers: {
        'Content-Type': 'application/json', 'Accept-Encoding': 'identity',
        Authorization: 'Bearer ' + provider.apiKey, 'Content-Length': bytes.length,
      },
    }, response => {
      if (response.statusCode >= 300 && response.statusCode < 400) {
        response.resume(); reject(Error('模型上游返回重定向，已停止请求。')); return;
      }
      const headers = new Headers();
      for (const key of ['content-type', 'cache-control', 'x-request-id', 'retry-after']) {
        const value = response.headers[key]; if (typeof value === 'string') headers.set(key, value);
      }
      try {
        const status = response.statusCode ?? 502;
        if (status === 204 || status === 205) {
          response.resume(); resolve(new Response(null, { status, headers }));
        } else resolve(new Response(Readable.toWeb(response), { status, headers }));
      } catch {
        response.resume(); reject(Error('模型上游响应无效，未回退到官方模型。'));
      }
    });
    request.on('error', () => reject(Error('Token Market 模型请求失败，未回退到官方模型。')));
    const abort = () => request.destroy(Error('模型请求已取消。'));
    if (signal?.aborted) { abort(); reject(Error('模型请求已取消。')); return; }
    signal?.addEventListener('abort', abort, { once: true });
    request.on('close', () => signal?.removeEventListener('abort', abort));
    request.setTimeout(120000, () => request.destroy(Error('模型上游响应超时。')));
    request.end(bytes);
  });
}
function createAdapter() {
  const nativeFetch = globalThis.fetch.bind(globalThis);
  let provider, failure, nativeQuota;
  try { provider = snapshot(); } catch (error) { failure = error.message || FAILURE; }
  const ready = () => { if (!provider || failure) throw Error(failure || FAILURE); return provider; };
  const decisionURLs = new Set();
  try {
    const decision = new URL(process.env.ROUTING_DECIDE_API_URL ?? 'http://dumate-meta-harness:8080/routing/decide');
    decisionURLs.add(decision.href);
    if (process.env.DUMATE_SCHEDULER_URL) decisionURLs.add(process.env.DUMATE_SCHEDULER_URL.replace(/\/+$/, '') + '/api/qianfanproxy' + decision.pathname + decision.search);
  } catch { failure = '搭子任务路由接口无法识别。'; }
  function nativeMedia(input, init, url) {
    if (url.origin !== new URL(VIRTUAL).origin) return nativeFetch(input, init);
    if (!nativeQuota?.baseURL) throw Error('无法确认图片或视频的原生模型入口。');
    const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
    headers.set('Authorization', 'Bearer ' + (nativeQuota.apiKey ?? 'nokey'));
    const target = nativeQuota.baseURL.replace(/\/+$/, '') + url.pathname.replace(/^\/v1(?=\/|$)/, '') + url.search;
    return nativeFetch(target, {
      ...init, headers, method: init?.method ?? (input instanceof Request ? input.method : 'GET'),
      body: init?.body ?? (input instanceof Request ? input.clone().body : undefined), duplex: 'half',
      signal: init?.signal ?? (input instanceof Request ? input.signal : undefined),
    });
  }
  async function unifiedFetch(input, init) {
    const url = new URL(input instanceof Request ? input.url : String(input));
    if (decisionURLs.has(url.href)) {
      ready();
      const body = JSON.parse(await bodyText(input, init));
      const media = mediaParts(body.parts);
      // In unified mode choosing a cloud model is not a classification task. Resolve
      // the vendor's preflight protocol locally, before it can call a cloud Agent.
      return new Response(JSON.stringify({ task_type: 'dumate', platform: 'desktop', model_type: media ? 'multimodal' : 'text', reason: 'fixed_model_selected', model_type_reason: media ? 'multimodal_file' : 'default_text' }), { headers: { 'Content-Type': 'application/json' } });
    }
    if (mediaPath.test(url.pathname)) return nativeMedia(input, init, url);
    const candidate = typeof init?.body === 'string' && init.body.includes('"model"') && init.body.includes('"messages"');
    if (cloudRun.test(url.pathname)) throw Error('当前请求仍选择了云端助手，已阻止官方模型回退。');
    if (!modelPath.test(url.pathname) && !candidate && url.origin !== new URL(VIRTUAL).origin) return nativeFetch(input, init);
    const selected = ready();
    if (!url.pathname.endsWith('/chat/completions')) throw Error('所选模型暂不支持此模型协议，已阻止官方模型回退。');
    let payload;
    try { payload = JSON.parse(await bodyText(input, init)); } catch { throw Error('模型请求格式无效，已阻止官方模型回退。'); }
    if (!Array.isArray(payload.messages)) throw Error('模型请求缺少 messages。');
    if (payload.model === (process.env.ROUTING_MULTIMODAL_MODEL_ID ?? 'model-mm') || payload.messages.some(message => mediaParts(message.content))) {
      return nativeMedia(input, init, url);
    }
    if (payload.messages.some(message => Array.isArray(message.content) && message.content.some(part => !['text', 'input_text'].includes(part.type)))) throw Error('当前文本接管不支持此输入格式。');
    payload.model = selected.model;
    if ('max_completion_tokens' in payload) {
      payload.max_completion_tokens = Math.min(selected.outputLimit, Number.isSafeInteger(payload.max_completion_tokens) && payload.max_completion_tokens > 0 ? payload.max_completion_tokens : selected.outputLimit);
      delete payload.max_tokens;
    } else payload.max_tokens = Math.min(selected.outputLimit, Number.isSafeInteger(payload.max_tokens) && payload.max_tokens > 0 ? payload.max_tokens : selected.outputLimit);
    return forward(selected, payload, init?.signal ?? (input instanceof Request ? input.signal : undefined));
  }
  // Scope is this DuMate engine only. The private gateway function always uses node:http(s)
  // directly; environment/system proxies and Baidu cookies never receive the platform key.
  globalThis.fetch = unifiedFetch;
  return {
    config(config) {
      config.model = 'QianfanPersonalQuota/glm-5'; config.small_model = config.model;
      for (const agent of Object.values(config.agent ?? {})) if (agent && typeof agent === 'object') agent.model = config.model;
      const quota = config.provider?.QianfanPersonalQuota;
      if (!quota) { failure = '搭子个人版模型接口发生变化。'; throw Error(failure); }
      if (quota.options?.baseURL !== VIRTUAL) nativeQuota = { ...quota.options };
      quota.options = { ...quota.options, baseURL: VIRTUAL, apiKey: 'tokenmarket-engine-local', fetch: unifiedFetch };
      const selected = ready();
      const others = (config.plugin ?? []).filter(item => !String(item).endsWith('/tokenmarket-dumate-unified.js'));
      if (others.length) { failure = '检测到其他引擎扩展，请先处理兼容性后重新接入。'; throw Error(failure); }
      for (const model of Object.values(quota.models ?? {})) model.limit = { ...model.limit, context: selected.contextLimit, output: selected.outputLimit };
    },
    message(_input, output) {
      const selected = ready();
      output.message.modelSettings = { ...output.message.modelSettings, modelSelection: {
        mode: 'fixed', modelKey: 'model-text', displayName: selected.model,
        limit: { context: selected.contextLimit, output: selected.outputLimit },
      } };
    },
    params(input) {
      if (input.model.providerID === 'qianfan-multimodal') return;
      const selected = ready();
      if (input.model.providerID !== 'QianfanPersonalQuota') throw Error('当前模型来源不支持统一接管，已阻止官方模型回退。');
      input.model.limit.output = Math.min(input.model.limit.output || selected.outputLimit, selected.outputLimit);
    },
  };
}
export default async function TokenMarketUnified() {
  const adapter = globalThis[SLOT] ??= createAdapter();
  return { config: config => adapter.config(config), 'chat.message': (input, output) => adapter.message(input, output), 'chat.params': input => adapter.params(input) };
}
