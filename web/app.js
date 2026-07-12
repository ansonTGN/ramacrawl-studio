'use strict';

const state = {
  loaded: false,
  running: false,
  config: null,
  pollTimer: null,
};

const byId = (id) => document.getElementById(id);
const form = byId('configForm');
const alertBox = byId('alert');
const scanButton = byId('scanButton');
const cancelButton = byId('cancelButton');
const dashboardButton = byId('dashboardButton');
const progressBar = byId('progressBar');

const ids = {
  seeds: byId('seeds'),
  allowedDomains: byId('allowedDomains'),
  deniedPaths: byId('deniedPaths'),
  allowedPorts: byId('allowedPorts'),
  allowSubdomains: byId('allowSubdomains'),
  allowPrivateNetworks: byId('allowPrivateNetworks'),
  respectRobots: byId('respectRobots'),
  robotsFailClosed: byId('robotsFailClosed'),
  discoverLinks: byId('discoverLinks'),
  robotsUserAgent: byId('robotsUserAgent'),
  linkSelector: byId('linkSelector'),
  includeRegex: byId('includeRegex'),
  excludeRegex: byId('excludeRegex'),
  maxPages: byId('maxPages'),
  maxDepth: byId('maxDepth'),
  concurrency: byId('concurrency'),
  perHostConcurrency: byId('perHostConcurrency'),
  minDelayMs: byId('minDelayMs'),
  timeoutSecs: byId('timeoutSecs'),
  retries: byId('retries'),
  redirectLimit: byId('redirectLimit'),
  maxBodyMb: byId('maxBodyMb'),
  userAgentProfile: byId('userAgentProfile'),
  customUserAgent: byId('customUserAgent'),
  customProfileHeaders: byId('customProfileHeaders'),
  extractionRules: byId('extractionRules'),
  sqlitePath: byId('sqlitePath'),
  jsonlPath: byId('jsonlPath'),
  summaryPath: byId('summaryPath'),
  bodiesDir: byId('bodiesDir'),
  saveBodies: byId('saveBodies'),
  customHeaders: byId('customHeaders'),
  proxies: byId('proxies'),
  websocketConfig: byId('websocketConfig'),
  cleanOutput: byId('cleanOutput'),
};

function lines(value) {
  return String(value || '')
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function objectToHeaderLines(value) {
  return Object.entries(value || {})
    .map(([name, headerValue]) => `${name}: ${headerValue}`)
    .join('\n');
}

function parseHeaderLines(value) {
  const headers = {};
  for (const line of lines(value)) {
    const separator = line.indexOf(':');
    if (separator < 1) {
      throw new Error(`Cabecera inválida: “${line}”. Usa el formato nombre: valor.`);
    }
    const name = line.slice(0, separator).trim();
    const headerValue = line.slice(separator + 1).trim();
    if (!name || !headerValue) {
      throw new Error(`Cabecera incompleta: “${line}”.`);
    }
    headers[name] = headerValue;
  }
  return headers;
}

function numberValue(element, name, { integer = true, min = 0 } = {}) {
  const value = Number(element.value);
  if (!Number.isFinite(value) || value < min || (integer && !Number.isInteger(value))) {
    element.classList.add('invalid');
    throw new Error(`${name} no contiene un número válido.`);
  }
  element.classList.remove('invalid');
  return value;
}

function optionalText(element) {
  const value = element.value.trim();
  return value || null;
}

function fillForm(config) {
  const crawler = config.crawler || {};
  const storage = config.storage || {};
  ids.seeds.value = (crawler.seeds || []).join('\n');
  ids.allowedDomains.value = (crawler.allowed_domains || []).join('\n');
  ids.deniedPaths.value = (crawler.denied_path_prefixes || []).join('\n');
  ids.allowedPorts.value = (crawler.allowed_ports || [80, 443]).join(', ');
  ids.allowSubdomains.checked = Boolean(crawler.allow_subdomains);
  ids.allowPrivateNetworks.checked = Boolean(crawler.allow_private_networks);
  ids.respectRobots.checked = crawler.respect_robots_txt !== false;
  ids.robotsFailClosed.checked = crawler.robots_fail_closed !== false;
  ids.discoverLinks.checked = crawler.discover_links !== false;
  ids.robotsUserAgent.value = crawler.robots_user_agent || 'RamaControlledExtractor';
  ids.linkSelector.value = crawler.link_selector || 'a[href]';
  ids.includeRegex.value = crawler.include_url_regex || '';
  ids.excludeRegex.value = crawler.exclude_url_regex || '';
  ids.maxPages.value = crawler.max_pages ?? 40;
  ids.maxDepth.value = crawler.max_depth ?? 2;
  ids.concurrency.value = crawler.concurrency ?? 2;
  ids.perHostConcurrency.value = crawler.per_host_concurrency ?? 1;
  ids.minDelayMs.value = crawler.min_delay_ms ?? 1000;
  ids.timeoutSecs.value = crawler.timeout_secs ?? 30;
  ids.retries.value = crawler.retries ?? 2;
  ids.redirectLimit.value = crawler.redirect_limit ?? 5;
  ids.maxBodyMb.value = ((crawler.max_body_bytes || 8_388_608) / 1_048_576).toFixed(1);
  ids.customHeaders.value = objectToHeaderLines(crawler.custom_headers);
  ids.proxies.value = (crawler.proxies || []).join('\n');

  const profile = crawler.user_agent_profile;
  if (profile && typeof profile === 'object' && profile.custom) {
    ids.userAgentProfile.value = 'custom';
    ids.customUserAgent.value = profile.custom.user_agent || '';
    ids.customProfileHeaders.value = objectToHeaderLines(profile.custom.headers);
  } else {
    ids.userAgentProfile.value = profile || 'minimal_bot';
    ids.customUserAgent.value = '';
    ids.customProfileHeaders.value = '';
  }
  updateCustomProfileVisibility();

  ids.extractionRules.value = JSON.stringify(config.extraction?.rules || [], null, 2);
  ids.sqlitePath.value = storage.sqlite_path || 'output/extractor.sqlite3';
  ids.jsonlPath.value = storage.jsonl_path || 'output/records.jsonl';
  ids.summaryPath.value = storage.summary_path || 'output/summary.json';
  ids.bodiesDir.value = storage.bodies_dir || 'output/bodies';
  ids.saveBodies.checked = Boolean(storage.save_bodies);
  ids.websocketConfig.value = JSON.stringify(config.websocket ?? null, null, 2);
  validateRulesEditor();
}

function readForm() {
  const seeds = lines(ids.seeds.value);
  const allowedDomains = lines(ids.allowedDomains.value);
  const ports = ids.allowedPorts.value
    .split(/[\s,;]+/)
    .filter(Boolean)
    .map((value) => Number(value));

  if (!seeds.length) {
    ids.seeds.classList.add('invalid');
    throw new Error('Debes introducir al menos una URL semilla.');
  }
  ids.seeds.classList.remove('invalid');
  for (const seed of seeds) {
    let parsed;
    try {
      parsed = new URL(seed);
    } catch {
      throw new Error(`URL semilla inválida: ${seed}`);
    }
    if (!['http:', 'https:'].includes(parsed.protocol)) {
      throw new Error(`La semilla debe usar HTTP o HTTPS: ${seed}`);
    }
  }
  if (!allowedDomains.length) {
    ids.allowedDomains.classList.add('invalid');
    throw new Error('Debes introducir al menos un dominio permitido.');
  }
  ids.allowedDomains.classList.remove('invalid');
  for (const domain of allowedDomains) {
    if (domain.includes('://') || domain.includes('/') || /\s/.test(domain)) {
      throw new Error(`Dominio permitido inválido: ${domain}`);
    }
  }
  if (!ports.length || ports.some((port) => !Number.isInteger(port) || port < 1 || port > 65535)) {
    ids.allowedPorts.classList.add('invalid');
    throw new Error('Los puertos deben ser números entre 1 y 65535.');
  }
  ids.allowedPorts.classList.remove('invalid');

  let extractionRules;
  try {
    extractionRules = JSON.parse(ids.extractionRules.value);
  } catch (error) {
    ids.extractionRules.classList.add('invalid');
    throw new Error(`El JSON de reglas de extracción no es válido: ${error.message}`);
  }
  if (!Array.isArray(extractionRules)) {
    throw new Error('Las reglas de extracción deben ser un array JSON.');
  }
  ids.extractionRules.classList.remove('invalid');

  let websocketConfig;
  try {
    websocketConfig = JSON.parse(ids.websocketConfig.value || 'null');
  } catch (error) {
    ids.websocketConfig.classList.add('invalid');
    throw new Error(`La configuración WebSocket no es JSON válido: ${error.message}`);
  }
  ids.websocketConfig.classList.remove('invalid');

  const profileName = ids.userAgentProfile.value;
  let userAgentProfile = profileName;
  if (profileName === 'custom') {
    const userAgent = ids.customUserAgent.value.trim();
    if (!userAgent) {
      throw new Error('El perfil personalizado requiere una cadena User-Agent.');
    }
    userAgentProfile = {
      custom: {
        user_agent: userAgent,
        headers: parseHeaderLines(ids.customProfileHeaders.value),
      },
    };
  }

  for (const proxy of lines(ids.proxies.value)) {
    let parsed;
    try {
      parsed = new URL(proxy);
    } catch {
      throw new Error(`Proxy inválido: ${proxy}`);
    }
    if (!['http:', 'https:', 'socks5:', 'socks5h:'].includes(parsed.protocol)) {
      throw new Error(`Esquema de proxy no admitido: ${proxy}`);
    }
  }

  const maxBodyMb = numberValue(ids.maxBodyMb, 'Cuerpo máximo', { integer: false, min: 0.1 });
  const config = {
    crawler: {
      seeds,
      allowed_domains: allowedDomains,
      denied_path_prefixes: lines(ids.deniedPaths.value),
      allowed_ports: [...new Set(ports)],
      allow_subdomains: ids.allowSubdomains.checked,
      allow_private_networks: ids.allowPrivateNetworks.checked,
      respect_robots_txt: ids.respectRobots.checked,
      robots_fail_closed: ids.robotsFailClosed.checked,
      robots_user_agent: ids.robotsUserAgent.value.trim() || 'RamaControlledExtractor',
      max_pages: numberValue(ids.maxPages, 'Máximo de páginas', { min: 1 }),
      max_depth: numberValue(ids.maxDepth, 'Profundidad máxima', { min: 0 }),
      concurrency: numberValue(ids.concurrency, 'Concurrencia global', { min: 1 }),
      per_host_concurrency: numberValue(ids.perHostConcurrency, 'Concurrencia por host', { min: 1 }),
      min_delay_ms: numberValue(ids.minDelayMs, 'Demora mínima', { min: 0 }),
      timeout_secs: numberValue(ids.timeoutSecs, 'Timeout', { min: 1 }),
      retries: numberValue(ids.retries, 'Reintentos', { min: 0 }),
      redirect_limit: numberValue(ids.redirectLimit, 'Límite de redirects', { min: 0 }),
      max_body_bytes: Math.round(maxBodyMb * 1_048_576),
      user_agent_profile: userAgentProfile,
      custom_headers: parseHeaderLines(ids.customHeaders.value),
      proxies: lines(ids.proxies.value),
      discover_links: ids.discoverLinks.checked,
      link_selector: ids.linkSelector.value.trim() || 'a[href]',
      include_url_regex: optionalText(ids.includeRegex),
      exclude_url_regex: optionalText(ids.excludeRegex),
    },
    extraction: { rules: extractionRules },
    storage: {
      sqlite_path: requiredPath(ids.sqlitePath, 'SQLite'),
      jsonl_path: requiredPath(ids.jsonlPath, 'JSONL'),
      summary_path: requiredPath(ids.summaryPath, 'Resumen JSON'),
      save_bodies: ids.saveBodies.checked,
      bodies_dir: requiredPath(ids.bodiesDir, 'Directorio de cuerpos'),
    },
    websocket: websocketConfig,
  };

  if (config.crawler.per_host_concurrency > config.crawler.concurrency) {
    throw new Error('La concurrencia por host no puede superar la concurrencia global.');
  }
  if (config.crawler.redirect_limit > 32) {
    throw new Error('El límite de redirects no puede superar 32.');
  }
  if (config.crawler.retries > 20) {
    throw new Error('Los reintentos no pueden superar 20.');
  }
  return config;
}

function requiredPath(element, label) {
  const value = element.value.trim();
  if (!value) {
    element.classList.add('invalid');
    throw new Error(`${label} requiere una ruta de salida.`);
  }
  element.classList.remove('invalid');
  return value;
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...(options.headers || {}),
    },
  });
  const text = await response.text();
  let payload = null;
  if (text) {
    try { payload = JSON.parse(text); } catch { payload = { error: text }; }
  }
  if (!response.ok) {
    throw new Error(payload?.error || `Error HTTP ${response.status}`);
  }
  return payload;
}

async function loadApplication() {
  try {
    const [config, status] = await Promise.all([api('/api/config'), api('/api/status')]);
    state.config = config;
    state.loaded = true;
    fillForm(config);
    updateStatus(status);
    setConnection(true);
    validateClientSide(false);
  } catch (error) {
    setConnection(false);
    showAlert(error.message);
  }
  schedulePoll();
}

function schedulePoll() {
  clearTimeout(state.pollTimer);
  state.pollTimer = setTimeout(async () => {
    try {
      const status = await api('/api/status');
      setConnection(true);
      updateStatus(status);
    } catch (error) {
      setConnection(false);
      console.error(error);
    } finally {
      schedulePoll();
    }
  }, state.running ? 850 : 2500);
}

function updateStatus(status) {
  const phase = status.phase || 'idle';
  state.running = phase === 'running';
  const labels = {
    idle: 'Escáner preparado',
    running: 'Escaneo en ejecución',
    completed: 'Escaneo completado',
    failed: 'Escaneo con errores',
    cancelled: 'Escaneo cancelado',
  };
  byId('scanTitle').textContent = labels[phase] || phase;
  byId('scanMessage').textContent = status.message || '';

  const progress = status.progress || status.summary || {};
  const processed = Number(progress.pages_fetched || 0)
    + Number(progress.pages_failed || 0)
    + Number(progress.pages_blocked || 0);
  const scheduled = Number(progress.pages_scheduled || 0);
  const maxPages = Number(progress.max_pages || scheduled || 0);
  const denominator = Math.max(scheduled, maxPages > 0 && phase === 'running' ? maxPages : scheduled, 1);
  const percent = phase === 'completed' ? 100 : Math.min(100, (processed / denominator) * 100);
  progressBar.style.width = `${percent}%`;
  progressBar.classList.toggle('indeterminate', state.running && scheduled === 0);
  const progressTrack = progressBar.parentElement;
  if (progressTrack) {
    progressTrack.setAttribute('aria-valuenow', String(Math.round(percent)));
    progressTrack.setAttribute('aria-valuetext', state.running && scheduled === 0
      ? 'Escaneo en curso, progreso indeterminado'
      : `${Math.round(percent)} por ciento`);
  }
  const scanCard = document.querySelector('.scan-card');
  if (scanCard) scanCard.setAttribute('aria-busy', String(state.running));
  byId('metricProcessed').textContent = processed.toLocaleString('es-ES');
  byId('metricScheduled').textContent = scheduled.toLocaleString('es-ES');
  byId('metricRecords').textContent = Number(progress.records_extracted || 0).toLocaleString('es-ES');
  byId('metricErrors').textContent = Number(progress.pages_failed || 0).toLocaleString('es-ES');
  byId('metricBytes').textContent = formatBytes(Number(progress.bytes_downloaded || 0));

  scanButton.disabled = !state.loaded || state.running || !validateClientSide(false);
  cancelButton.classList.toggle('hidden', !state.running);
  dashboardButton.classList.toggle('hidden', !status.dashboard_available);
  byId('saveButton').disabled = state.running;
  byId('validateButton').disabled = state.running;
  form.querySelectorAll('input,textarea,select,button').forEach((element) => {
    if (!['cancelButton'].includes(element.id)) {
      element.disabled = state.running && !['scanButton'].includes(element.id);
    }
  });
  scanButton.disabled = state.running || !validateClientSide(false);
}

function setConnection(online) {
  const badge = byId('connectionBadge');
  badge.classList.toggle('online', online);
  badge.classList.toggle('offline', !online);
  badge.lastChild.textContent = online ? ' Conectado' : ' Sin conexión';
}

function showAlert(message, type = 'error') {
  alertBox.textContent = message;
  alertBox.className = `alert${type === 'success' ? ' success' : ''}`;
  alertBox.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
}

function hideAlert() {
  alertBox.className = 'alert hidden';
  alertBox.textContent = '';
}

function validateClientSide(showMessage = true) {
  try {
    readForm();
    if (showMessage) showAlert('La configuración es coherente en el navegador.', 'success');
    return true;
  } catch (error) {
    if (showMessage) showAlert(error.message);
    return false;
  }
}

async function validateOnServer() {
  hideAlert();
  try {
    const config = readForm();
    const result = await api('/api/validate', {
      method: 'POST',
      body: JSON.stringify(config),
    });
    showAlert(result.message, 'success');
    scanButton.disabled = state.running;
  } catch (error) {
    showAlert(error.message);
  }
}

async function saveOnServer() {
  hideAlert();
  try {
    const config = readForm();
    const result = await api('/api/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
    state.config = config;
    showAlert(`${result.message} ${result.config_path}`, 'success');
  } catch (error) {
    showAlert(error.message);
  }
}

async function startScan() {
  hideAlert();
  try {
    const config = readForm();
    scanButton.disabled = true;
    const status = await api('/api/scan', {
      method: 'POST',
      body: JSON.stringify({ config, clean_output: ids.cleanOutput.checked }),
    });
    state.config = config;
    updateStatus(status);
    showAlert(ids.cleanOutput.checked
      ? 'Configuración guardada, salidas anteriores eliminadas y escaneo iniciado.'
      : 'Configuración guardada. El escaneo ha comenzado.', 'success');
  } catch (error) {
    showAlert(error.message);
    scanButton.disabled = !validateClientSide(false);
  }
}

async function cancelScan() {
  hideAlert();
  try {
    const status = await api('/api/scan/cancel', { method: 'POST', body: '{}' });
    updateStatus(status);
    showAlert('El escaneo se ha cancelado.', 'success');
  } catch (error) {
    showAlert(error.message);
  }
}

function updateCustomProfileVisibility() {
  byId('customUserAgentFields').classList.toggle('hidden', ids.userAgentProfile.value !== 'custom');
}

function validateRulesEditor() {
  const status = byId('rulesStatus');
  try {
    const parsed = JSON.parse(ids.extractionRules.value);
    if (!Array.isArray(parsed)) throw new Error('Debe ser un array');
    status.textContent = `${parsed.length} regla${parsed.length === 1 ? '' : 's'} válida${parsed.length === 1 ? '' : 's'}`;
    status.className = 'editor-status valid';
    ids.extractionRules.classList.remove('invalid');
    return true;
  } catch (error) {
    status.textContent = error.message;
    status.className = 'editor-status invalid';
    return false;
  }
}

function formatRules() {
  try {
    ids.extractionRules.value = JSON.stringify(JSON.parse(ids.extractionRules.value), null, 2);
    validateRulesEditor();
  } catch (error) {
    showAlert(`No se puede formatear: ${error.message}`);
  }
}

function loadArticlePreset() {
  const preset = [
    {
      kind: 'html_records',
      name: 'articles',
      item_selector: 'article, .elementor-post, .post-item',
      fields: {
        title: { selector: '.elementor-post__title a, .entry-title a, h2 a, h3 a', trim: true },
        url: { selector: '.elementor-post__title a[href], .entry-title a[href], h2 a[href], h3 a[href]', attribute: 'href', trim: true },
        date: { selector: 'time, .elementor-post-date, .entry-date', trim: true },
        summary: { selector: '.elementor-post__excerpt, .entry-summary, p', trim: true },
        image_url: { selector: 'img', attribute: 'src', trim: true },
      },
    },
  ];
  ids.extractionRules.value = JSON.stringify(preset, null, 2);
  validateRulesEditor();
}

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / (1024 ** index)).toLocaleString('es-ES', { maximumFractionDigits: 1 })} ${units[index]}`;
}

const navigationButtons = [...document.querySelectorAll('.nav-button')];
const configurationPanels = [...document.querySelectorAll('[data-panel]')];

for (const button of navigationButtons) {
  const targetPanel = byId(button.dataset.target);
  button.setAttribute('aria-controls', button.dataset.target);
  button.setAttribute('aria-current', button.classList.contains('active') ? 'step' : 'false');
  if (targetPanel) targetPanel.hidden = !targetPanel.classList.contains('active');

  button.addEventListener('click', () => {
    navigationButtons.forEach((item) => {
      item.classList.remove('active');
      item.setAttribute('aria-current', 'false');
    });
    configurationPanels.forEach((panel) => {
      panel.classList.remove('active');
      panel.hidden = true;
    });
    button.classList.add('active');
    button.setAttribute('aria-current', 'step');
    targetPanel.classList.add('active');
    targetPanel.hidden = false;
    targetPanel.focus({ preventScroll: true });
    window.scrollTo({ top: 0, behavior: 'smooth' });
  });
}

ids.userAgentProfile.addEventListener('change', updateCustomProfileVisibility);
ids.extractionRules.addEventListener('input', validateRulesEditor);
byId('formatRulesButton').addEventListener('click', formatRules);
byId('articlePresetButton').addEventListener('click', loadArticlePreset);
byId('validateButton').addEventListener('click', validateOnServer);
byId('saveButton').addEventListener('click', saveOnServer);
scanButton.addEventListener('click', startScan);
cancelButton.addEventListener('click', cancelScan);
form.addEventListener('input', () => {
  if (!state.running) scanButton.disabled = !validateClientSide(false);
});
form.addEventListener('change', () => {
  if (!state.running) scanButton.disabled = !validateClientSide(false);
});

loadApplication();
