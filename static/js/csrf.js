// ─── CSRF Token Auto-Injection (REM-4a) ─────────────────────────
// Reads the homun_csrf cookie and injects X-CSRF-Token header
// into all non-GET/HEAD/OPTIONS fetch requests automatically.
'use strict';

(function () {
    var originalFetch = window.fetch;

    function getCsrfToken() {
        var match = document.cookie.match(/(?:^|;\s*)homun_csrf=([^;]+)/);
        return match ? match[1] : '';
    }

    window.fetch = function (input, init) {
        init = init || {};
        var method = (init.method || 'GET').toUpperCase();

        // Only inject CSRF for state-changing methods
        if (method !== 'GET' && method !== 'HEAD' && method !== 'OPTIONS') {
            var token = getCsrfToken();
            if (token) {
                var headers = new Headers(init.headers || {});
                if (!headers.has('X-CSRF-Token')) {
                    headers.set('X-CSRF-Token', token);
                }
                init.headers = headers;
            }
        }

        return originalFetch.call(this, input, init);
    };
})();
