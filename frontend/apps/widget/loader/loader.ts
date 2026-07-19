/* eslint-disable */
(function () {
  'use strict';

  var GUARD_KEY = '__hx_widget_loaded';
  var WIDGET_ORIGIN = (function () {
    var s = document.currentScript;
    if (!s) return '';
    var src = s.getAttribute('src') || '';
    try {
      var u = new URL(src, window.location.href);
      return u.origin;
    } catch {
      return '';
    }
  })();

  try {
    if (typeof window !== 'undefined') {
      if (window[GUARD_KEY]) return;
      window[GUARD_KEY] = true;
    }

    var scriptTag = document.currentScript;
    if (!scriptTag) return;
    var widgetId = scriptTag.getAttribute('data-widget-id');
    if (!widgetId) return;

    var state = { iframe: null, open: false };

    function getConfig(wid, cb) {
      var xhr = new XMLHttpRequest();
      xhr.open(
        'GET',
        WIDGET_ORIGIN + '/widget/v1/config?widgetId=' + encodeURIComponent(wid),
        true,
      );
      xhr.onload = function () {
        if (xhr.status === 404 || xhr.status === 403) {
          cb(null);
          return;
        }
        try {
          var body = JSON.parse(xhr.responseText);
          if (!body.data || body.data.enabled === false) {
            cb(null);
            return;
          }
          cb(body.data);
        } catch {
          cb(null);
        }
      };
      xhr.onerror = function () {
        cb(null);
      };
      xhr.send();
    }

    function createLauncher(cfg) {
      var btn = document.createElement('button');
      btn.setAttribute('aria-label', 'Open chat');

      var isRight = cfg.position !== 'bottom-left';
      btn.style.cssText =
        'position:fixed;' +
        (isRight ? 'right:20px;' : 'left:20px;') +
        'bottom:20px;' +
        'width:56px;height:56px;' +
        'border-radius:50%;' +
        'border:none;' +
        'background:' +
        cfg.primaryColor +
        ';' +
        'color:#fff;' +
        'cursor:pointer;' +
        'z-index:2147483647;' +
        'display:flex;' +
        'align-items:center;' +
        'justify-content:center;' +
        'box-shadow:0 4px 12px rgba(0,0,0,0.2);' +
        'transition:transform 0.15s;';

      btn.innerHTML =
        '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>' +
        '</svg>';

      btn.addEventListener('mouseenter', function () {
        btn.style.transform = 'scale(1.08)';
      });
      btn.addEventListener('mouseleave', function () {
        btn.style.transform = 'scale(1)';
      });

      btn.addEventListener('click', function () {
        toggleIframe(cfg, btn);
      });

      document.body.appendChild(btn);
    }

    function toggleIframe(cfg, btn) {
      if (!state.iframe) {
        state.iframe = document.createElement('iframe');
        state.iframe.setAttribute('title', 'Chat widget');
        state.iframe.setAttribute('role', 'dialog');
        state.iframe.setAttribute('aria-label', 'Chat widget');
        state.iframe.style.cssText =
          'position:fixed;' +
          (cfg.position !== 'bottom-left' ? 'right:20px;' : 'left:20px;') +
          'bottom:88px;' +
          'width:380px;' +
          'height:600px;' +
          'border:0;' +
          'border-radius:12px;' +
          'box-shadow:0 4px 24px rgba(0,0,0,0.15);' +
          'z-index:2147483646;' +
          'background:transparent;';
        state.iframe.src = WIDGET_ORIGIN + '/widget/?id=' + encodeURIComponent(cfg.widgetId);
        document.body.appendChild(state.iframe);
        state.open = true;
        btn.style.display = 'none';
      } else {
        if (state.open) {
          state.iframe.style.display = 'none';
          btn.style.display = 'flex';
          state.open = false;
        } else {
          state.iframe.style.display = 'block';
          btn.style.display = 'none';
          state.open = true;
        }
      }
    }

    function handleMessage(event) {
      if (event.origin !== WIDGET_ORIGIN) return;
      if (!event.data || event.data.source !== 'hx-widget') return;

      if (event.data.type === 'close') {
        if (state.iframe) state.iframe.style.display = 'none';
        state.open = false;
        var btns = document.querySelectorAll('button[aria-label="Open chat"]');
        btns.forEach(function (b) {
          b.style.display = 'flex';
        });
        if (btns.length > 0) btns[0].focus();
      }

      if (event.data.type === 'resize') {
        if (state.iframe) {
          var w = Math.max(320, Math.min(420, event.data.width || 380));
          var h = Math.max(300, Math.min(700, event.data.height || 600));
          state.iframe.style.width = w + 'px';
          state.iframe.style.height = h + 'px';
        }
      }
    }

    window.addEventListener('message', handleMessage);

    getConfig(widgetId, function (cfg) {
      if (!cfg) return;
      createLauncher(cfg);
    });
  } catch (_e) {
    // silent — never throw into host page
  }
})();
