(() => {
  const bytes = value => {
    if (value < 1024) return `${value} B`;
    const units = ['KiB','MiB','GiB'];
    let unit = -1;
    do {value /= 1024;unit++;} while (value >= 1024 && unit < units.length - 1);
    return `${value.toFixed(1)} ${units[unit]}`;
  };
  const values = progress => ({
    downloaded: Math.max(0, Number(progress?.downloadedBytes) || 0),
    total: Number(progress?.totalBytes) > 0 ? Number(progress.totalBytes) : null
  });
  const percent = ({downloaded,total}) => Math.min(100, Math.floor(downloaded / total * 100));
  function summary(progress) {
    const data = values(progress);
    return data.total ? `${percent(data)}% · ${bytes(data.downloaded)} / ${bytes(data.total)}` : data.downloaded ? `已下载 ${bytes(data.downloaded)} · 总大小未知` : '正在连接下载服务器…';
  }
  function render(container, active, progress) {
    container.hidden = !active;
    const bar = container.querySelector('progress');
    const data = values(progress);
    if (active && data.total) bar.value = percent(data);
    else bar.removeAttribute('value');
    container.querySelector('.tool-download-summary').textContent = active ? summary(progress) : '';
  }
  window.TokenMarketDownloadProgress = {render,summary};
})();
