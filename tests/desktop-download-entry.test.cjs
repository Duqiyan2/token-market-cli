// Real local HTTP attachments exercise browser downloads without OSS writes.
const {test, before, after} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const {chromium, webkit} = require('playwright');
const template = fs.readFileSync(path.join(__dirname, '../scripts/release-assets/download.html'), 'utf8');
let browser, server, origin, downloads = 0;

before(async () => {
  browser = await (process.env.TOKEN_MARKET_TEST_BROWSER_ENGINE === 'webkit' ? webkit : chromium).launch({
    headless: true, ...(process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL ? {channel: process.env.TOKEN_MARKET_TEST_BROWSER_CHANNEL} : {})
  });
  server = http.createServer((request, response) => {
    if (request.url === '/download') {
      response.setHeader('Content-Type', 'text/html; charset=utf-8');
      response.end(template.replaceAll('__DOWNLOAD_ORIGIN__', origin));
    } else if (['/TokenMarket-mac-arm64.dmg', '/TokenMarket-windows-x64.exe'].includes(request.url)) {
      downloads++;
      response.setHeader('Content-Type', 'application/octet-stream');
      response.setHeader('Content-Disposition', `attachment; filename="${request.url.slice(1)}"`);
      response.end('inert-downloaded-fixture');
    } else {
      response.setHeader('Content-Type', 'text/html; charset=utf-8');
      response.end('<a href="/download">下载客户端</a>');
    }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
after(async () => {
  await browser?.close();
  if (server) await new Promise(resolve => server.close(resolve));
});

const scenarios = [
  {name: 'Windows', platform: 'Win32', hint: 'Windows', ua: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', touch: 0, file: 'TokenMarket-windows-x64.exe'},
  {name: 'Mac', platform: 'MacIntel', hint: null, ua: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', touch: 0, file: 'TokenMarket-mac-arm64.dmg'},
  {name: 'Android', platform: 'Linux armv8l', hint: 'Android', ua: 'Mozilla/5.0 (Linux; Android 14) Mobile', touch: 5},
  {name: 'iPad desktop mode', platform: 'MacIntel', hint: null, ua: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)', touch: 5},
  {name: 'Linux', platform: 'Linux x86_64', hint: 'Linux', ua: 'Mozilla/5.0 (X11; Linux x86_64)', touch: 0}
];
for (const scenario of scenarios) test(`website download entry handles ${scenario.name}`, async () => {
  downloads = 0;
  const context = await browser.newContext({userAgent: scenario.ua, acceptDownloads: true, viewport: {width: 390, height: 800}});
  try {
    await context.addInitScript(value => {
      Object.defineProperty(navigator, 'platform', {value: value.platform});
      Object.defineProperty(navigator, 'maxTouchPoints', {value: value.touch});
      Object.defineProperty(navigator, 'userAgentData', {value: value.hint ? {platform: value.hint} : undefined});
    }, scenario);
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(origin);
    if (scenario.file) {
      const pending = page.waitForEvent('download');
      await page.getByRole('link', {name: '下载客户端', exact: true}).click();
      const download = await pending;
      assert.equal(download.url(), origin + '/' + scenario.file);
      assert.equal(download.suggestedFilename(), scenario.file);
      assert.equal(fs.readFileSync(await download.path(), 'utf8'), 'inert-downloaded-fixture');
      assert.equal(downloads, 1);
      await download.delete();
    } else {
      await page.getByRole('link', {name: '下载客户端', exact: true}).click();
      await page.getByRole('status').filter({hasText: '请选择需要下载的桌面安装包'}).waitFor();
      assert.equal(downloads, 0);
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
      if (scenario.name === 'Linux') {
        const pending = page.waitForEvent('download');
        await page.getByRole('link', {name: /下载 Windows 版/}).click();
        assert.equal((await pending).suggestedFilename(), 'TokenMarket-windows-x64.exe');
      }
    }
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
  }
});
