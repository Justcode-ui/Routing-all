// ROUTINGALL — GUI logic & Tauri IPC bridge
// All Tauri IPC calls degrade gracefully to mock data when running outside Tauri
(function () {
  'use strict';

  // ── SVG Icons ────────────────────────────────────────────────────────────────
  const EYE_ICON_SVG = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path><circle cx="12" cy="12" r="3"></circle></svg>';
  const EYE_OFF_ICON_SVG = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path><line x1="1" y1="1" x2="23" y2="23"></line></svg>';

  // ── State ────────────────────────────────────────────────────────────────────
  let masterKey = 'rg-master-key-7f2a89c104e2abc1';
  let isMasterKeyRevealed = false;
  let activeTab = 'python';
  let currentEditingProvider = null;
  const PROXY_PORT = 8081;

  const providerState = { groq: null, gemini: null, openrouter: null };
  const providerPlaceholders = {
    groq: 'gsk_...',
    gemini: 'AIzaSy...',
    openrouter: 'sk-or-v1-...',
  };
  const providerLabels = {
    groq: 'Groq',
    gemini: 'Google AI Studio',
    openrouter: 'OpenRouter',
  };

  // ── Tauri IPC bridge ─────────────────────────────────────────────────────────
  async function invoke(cmd, args) {
    args = args || {};
    if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
      return await window.__TAURI__.core.invoke(cmd, args);
    }
    // Mock mode for standalone browser preview
    if (cmd === 'get_master_key') return masterKey;
    if (cmd === 'rotate_master_key') {
      masterKey = 'rg-master-key-' + Math.random().toString(36).slice(2, 18);
      return masterKey;
    }
    if (cmd === 'save_provider_key') {
      providerState[args.provider] = args.key;
      return null;
    }
    if (cmd === 'remove_all_keys') {
      Object.keys(providerState).forEach(function(k) { providerState[k] = null; });
      masterKey = 'rg-master-key-' + Math.random().toString(36).slice(2, 18);
      return null;
    }
    if (cmd === 'get_health_status') {
      return {
        status: 'ok',
        version: '1.6.0',
        port: PROXY_PORT,
        is_listening: true,
        error: null,
        providers_configured: {
          groq: !!providerState.groq,
          gemini: !!providerState.gemini,
          openrouter: !!providerState.openrouter,
        },
        keychain_access: 'ok',
        usage: {
          groq: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
          gemini: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
          openrouter: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
        },
      };
    }
    if (cmd === 'get_usage_snapshot') {
      return {
        groq: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
        gemini: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
        openrouter: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
      };
    }
    if (cmd === 'quit_app') return null;
    return null;
  }

  // ── DOM refs ─────────────────────────────────────────────────────────────────
  function $(id) { return document.getElementById(id); }

  // ── Helpers ──────────────────────────────────────────────────────────────────
  function maskKey(key) {
    if (!key || key.length < 8) return '••••••••';
    return key.slice(0, 4) + '••••••••' + key.slice(-4);
  }

  function displayMasterKey() {
    if (isMasterKeyRevealed) return masterKey;
    return masterKey.slice(0, 14) + '••••' + masterKey.slice(-4);
  }

  function getExampleModel() {
    var first = Object.entries(providerState).find(function(e) { return e[1]; });
    if (!first) return 'groq/llama-3.1-70b-versatile';
    var p = first[0];
    if (p === 'groq') return 'groq/llama-3.1-70b-versatile';
    if (p === 'gemini') return 'gemini/gemini-1.5-pro';
    return 'openrouter/anthropic/claude-3-haiku';
  }

  function getSnippetForTab(tab) {
    var k = isMasterKeyRevealed ? masterKey : displayMasterKey();
    var model = getExampleModel();
    var port = PROXY_PORT;

    if (tab === 'python') {
      return 'from openai import OpenAI\n\nclient = OpenAI(\n    base_url="http://127.0.0.1:' + port + '/v1",\n    api_key="' + k + '"\n)\n\nresponse = client.chat.completions.create(\n    model="' + model + '",\n    messages=[{"role": "user", "content": "Hello"}]\n)';
    }
    if (tab === 'node') {
      return 'import OpenAI from "openai";\n\nconst client = new OpenAI({\n  baseURL: "http://127.0.0.1:' + port + '/v1",\n  apiKey: "' + k + '",\n});\n\nconst res = await client.chat.completions.create({\n  model: "' + model + '",\n  messages: [{ role: "user", content: "Hello" }],\n});';
    }
    if (tab === 'curl') {
      return 'curl http://127.0.0.1:' + port + '/v1/chat/completions \\\n  -H "Authorization: Bearer ' + k + '" \\\n  -H "Content-Type: application/json" \\\n  -d \'{\n    "model": "' + model + '",\n    "messages": [{"role":"user","content":"Hello"}]\n  }\'';
    }
    return '';
  }

  // ── Bulletproof Clipboard Copy Helper ────────────────────────────────────────
  async function copyToClipboard(text) {
    if (!text) return false;
    if (navigator.clipboard && navigator.clipboard.writeText) {
      try {
        await navigator.clipboard.writeText(text);
        return true;
      } catch (e) {
        // Fallback to execCommand
      }
    }
    try {
      var textarea = document.createElement('textarea');
      textarea.value = text;
      textarea.setAttribute('readonly', '');
      textarea.style.position = 'fixed';
      textarea.style.left = '-9999px';
      textarea.style.top = '-9999px';
      document.body.appendChild(textarea);
      textarea.select();
      var successful = document.execCommand('copy');
      document.body.removeChild(textarea);
      return successful;
    } catch (err) {
      return false;
    }
  }

  function doCopyMasterKey() {
    copyToClipboard(masterKey);
    var keyField = $('masterKeyDisplay');
    var copyBtn = $('copyMasterKeyBtn');
    var hint = $('copyKeyHint');

    if (keyField) keyField.classList.add('copied');
    if (copyBtn) copyBtn.classList.add('copied');
    if (hint) {
      hint.textContent = 'COPIED TO CLIPBOARD!';
      hint.style.color = 'var(--ok)';
    }

    setTimeout(function() {
      if (keyField) keyField.classList.remove('copied');
      if (copyBtn) copyBtn.classList.remove('copied');
      if (hint) {
        hint.textContent = 'Click key or icon to copy';
        hint.style.color = 'var(--text-dim)';
      }
    }, 1800);
  }

  // ── Render ───────────────────────────────────────────────────────────────────
  function renderMasterKey() {
    $('masterKeyDisplay').textContent = displayMasterKey();
    var toggleBtn = $('toggleMasterKeyBtn');
    if (toggleBtn) {
      toggleBtn.innerHTML = isMasterKeyRevealed ? EYE_OFF_ICON_SVG : EYE_ICON_SVG;
      toggleBtn.title = isMasterKeyRevealed ? 'Hide Master Key' : 'Show Master Key';
    }
    renderSnippet();
  }

  function renderSnippet() {
    $('snippetCode').textContent = getSnippetForTab(activeTab);
  }

  function renderProviders() {
    var count = 0;
    var map = { groq: 'Groq', gemini: 'Gemini', openrouter: 'OpenRouter' };
    Object.keys(map).forEach(function(p) {
      var suffix = map[p];
      var dot = $('dot' + suffix);
      var sub = $('sub' + suffix);
      var btn = $('edit' + suffix + 'Btn');
      if (!dot) return;
      if (providerState[p]) {
        count++;
        dot.className = 'status-dot ok';
        sub.textContent = maskKey(providerState[p]);
        btn.innerHTML = '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>';
        btn.style.color = '';
        btn.style.borderColor = '';
      } else {
        dot.className = 'status-dot off';
        sub.textContent = 'Not configured';
        btn.textContent = '+';
        btn.style.color = 'var(--signal)';
        btn.style.borderColor = 'rgba(255,122,51,0.35)';
      }
    });
    $('providerCount').textContent = count + ' of 3';
    renderSnippet();
  }

  function renderActivity(usage) {
    if (!usage) return;

    var chartContainer = $('activityChart');
    if (!chartContainer) return;

    var providers = [
      { id: 'groq', label: 'Groq', suffix: 'Groq' },
      { id: 'gemini', label: 'Google AI Studio', suffix: 'Gemini' },
      { id: 'openrouter', label: 'OpenRouter', suffix: 'OpenRouter' }
    ];

    var maxRequests = 0;
    providers.forEach(function(p) {
      var data = usage[p.id] || { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null };
      var count = (data.requests_this_session !== undefined ? data.requests_this_session : data.requests_last_hour) || 0;
      if (count > maxRequests) {
        maxRequests = count;
      }
    });

    var html = '';
    providers.forEach(function(p) {
      var data = usage[p.id] || { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null };
      var count = (data.requests_this_session !== undefined ? data.requests_this_session : data.requests_last_hour) || 0;
      var pct = maxRequests > 0 ? Math.round((count / maxRequests) * 100) : 0;
      var hasDataClass = count > 0 ? ' has-data' : '';
      var unit = count === 1 ? 'req' : 'reqs';

      html += '<div class="activity-row">' +
        '<div class="activity-label">' + p.label + '</div>' +
        '<div class="activity-track">' +
          '<div class="activity-bar' + hasDataClass + '" style="width: ' + pct + '%;"></div>' +
        '</div>' +
        '<div class="activity-count">' + count + ' ' + unit + '</div>' +
      '</div>';

      // Update rate-limit badge in provider row (FR-8.4)
      var badge = $('rlBadge' + p.suffix);
      if (badge) {
        if (data.rate_limit_remaining !== null && data.rate_limit_remaining !== undefined) {
          badge.textContent = data.rate_limit_remaining + ' rem';
          badge.classList.remove('hidden');
        } else {
          badge.classList.add('hidden');
        }
      }
    });

    chartContainer.innerHTML = html;
  }

  async function refreshHealth() {
    try {
      var h = await invoke('get_health_status');
      var statusPill = $('statusPill');
      var statusLabel = $('statusLabel');
      var conflictBanner = $('conflictBanner');
      var conflictText = $('conflictText');

      if (h) {
        $('portDisplay').textContent = 'Port ' + (h.port || PROXY_PORT);
        $('keychainDisplay').textContent = h.keychain_access === 'ok' ? 'Keychain OK' : 'Keychain error';
        $('endpointUrl').textContent = '127.0.0.1:' + (h.port || PROXY_PORT) + '/v1';

        if (h.is_listening) {
          statusPill.className = 'status-pill';
          statusLabel.textContent = 'LISTENING';
          conflictBanner.classList.add('hidden');
        } else {
          statusPill.className = 'status-pill error';
          statusLabel.textContent = 'PORT CONFLICT';
          conflictBanner.classList.remove('hidden');
          if (h.error) {
            conflictText.textContent = h.error + '. Please terminate any process using port ' + h.port + ' and restart.';
          }
        }

        if (h.usage) {
          renderActivity(h.usage);
        }
      }
    } catch (e) {
      /* proxy not yet started */
    }
  }

  // ── Copy flash helper ────────────────────────────────────────────────────────
  function flashCopied(btn, original) {
    btn.textContent = 'COPIED';
    setTimeout(function() { btn.textContent = original; }, 1500);
  }

  // ── Modal helpers ────────────────────────────────────────────────────────────
  function showModal(id) { $(id).classList.remove('hidden'); }
  function hideModal(id) { $(id).classList.add('hidden'); }

  // ── Event bindings ───────────────────────────────────────────────────────────

  // Copy Master Key triggers
  $('copyMasterKeyBtn').addEventListener('click', doCopyMasterKey);
  $('masterKeyDisplay').addEventListener('click', doCopyMasterKey);

  // Copy endpoint
  $('copyEndpointBtn').addEventListener('click', function() {
    copyToClipboard('http://127.0.0.1:' + PROXY_PORT + '/v1');
    flashCopied($('copyEndpointBtn'), 'COPY');
  });

  // Copy snippet
  $('copySnippetBtn').addEventListener('click', function() {
    copyToClipboard($('snippetCode').textContent);
    flashCopied($('copySnippetBtn'), 'COPY');
  });

  // Toggle Master Key reveal
  $('toggleMasterKeyBtn').addEventListener('click', function() {
    isMasterKeyRevealed = !isMasterKeyRevealed;
    renderMasterKey();
  });

  // Code tabs
  document.querySelectorAll('.snippet-tab').forEach(function(tab) {
    tab.addEventListener('click', function() {
      document.querySelectorAll('.snippet-tab').forEach(function(t) { t.classList.remove('active'); });
      tab.classList.add('active');
      activeTab = tab.dataset.tab;
      renderSnippet();
    });
  });

  // Regen Master Key modal
  $('regenMasterKeyBtn').addEventListener('click', function() { showModal('regenModal'); });
  $('cancelRegenBtn').addEventListener('click', function() { hideModal('regenModal'); });
  $('confirmRegenBtn').addEventListener('click', async function() {
    var newKey = await invoke('rotate_master_key');
    if (newKey) masterKey = newKey;
    hideModal('regenModal');
    renderMasterKey();
  });

  // Reset all keys modal — triggered from footer
  $('cancelResetBtn').addEventListener('click', function() { hideModal('resetModal'); });
  $('confirmResetBtn').addEventListener('click', async function() {
    await invoke('remove_all_keys');
    Object.keys(providerState).forEach(function(k) { providerState[k] = null; });
    var newKey = await invoke('get_master_key');
    if (newKey) masterKey = newKey;
    hideModal('resetModal');
    renderMasterKey();
    renderProviders();
  });

  // Quit
  $('quitBtn').addEventListener('click', function() { invoke('quit_app'); });

  // Provider edit buttons
  var providerMap = { Groq: 'groq', Gemini: 'gemini', OpenRouter: 'openrouter' };
  Object.keys(providerMap).forEach(function(label) {
    var pKey = providerMap[label];
    var btn = $('edit' + label + 'Btn');
    if (!btn) return;
    btn.addEventListener('click', function() {
      currentEditingProvider = pKey;
      $('modalProviderTitle').textContent = (providerState[pKey] ? 'Update ' : 'Add ') + providerLabels[pKey] + ' Key';
      $('modalProviderHelp').textContent = 'Paste your ' + providerLabels[pKey] + ' API key. Stored securely in your OS keychain.';
      var input = $('modalKeyInput');
      input.placeholder = providerPlaceholders[pKey];
      input.value = '';
      $('keyFormatWarning').textContent = '';
      showModal('editKeyModal');
      setTimeout(function() { input.focus(); }, 50);
    });
  });

  // Format validation on key input (PRD FR-5 — soft warning, never hard block)
  $('modalKeyInput').addEventListener('input', function() {
    if (!currentEditingProvider) return;
    var key = $('modalKeyInput').value.trim();
    var warning = $('keyFormatWarning');
    var prefixes = { groq: 'gsk_', gemini: 'AIzaSy', openrouter: 'sk-or-v1-' };
    var prefix = prefixes[currentEditingProvider];
    if (key && !key.startsWith(prefix)) {
      warning.textContent = "This doesn't look like a " + providerLabels[currentEditingProvider] + ' key';
    } else {
      warning.textContent = '';
    }
  });

  $('cancelKeyModalBtn').addEventListener('click', function() {
    hideModal('editKeyModal');
    currentEditingProvider = null;
  });

  $('saveKeyModalBtn').addEventListener('click', async function() {
    var key = $('modalKeyInput').value.trim();
    if (!key || !currentEditingProvider) return;
    await invoke('save_provider_key', { provider: currentEditingProvider, key: key });
    providerState[currentEditingProvider] = key;
    hideModal('editKeyModal');
    currentEditingProvider = null;
    renderProviders();
  });

  // ── Init ──────────────────────────────────────────────────────────────────────
  async function init() {
    var savedKey = await invoke('get_master_key');
    if (savedKey) masterKey = savedKey;
    renderMasterKey();
    renderProviders();
    renderActivity({
      groq: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
      gemini: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
      openrouter: { requests_this_session: 0, requests_last_hour: 0, rate_limit_remaining: null },
    });
    await refreshHealth();
    setInterval(refreshHealth, 10000);

    // Listen for tray Copy Master Key event
    if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) {
      window.__TAURI__.event.listen('copy_master_key', function() {
        doCopyMasterKey();
      });
    }
  }

  init();
})();
