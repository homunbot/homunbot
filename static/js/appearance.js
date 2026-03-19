/**
 * Appearance page — theme, language, accent color
 */

(function() {
    var appearanceForm = document.getElementById('appearance-form');
    var themeSelect = document.getElementById('theme-select');
    var languageSelect = document.getElementById('language-select');

    if (appearanceForm) {
        appearanceForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            var btn = appearanceForm.querySelector('button[type="submit"]');
            var originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;

            var theme = themeSelect ? themeSelect.value : 'system';
            var language = languageSelect ? languageSelect.value : 'system';
            var accent = localStorage.getItem('homun-accent') || '';
            var texture = localStorage.getItem('homun-texture') || 'none';

            try {
                var responses = await Promise.all([
                    fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ key: 'ui.theme', value: theme }),
                    }),
                    fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ key: 'ui.language', value: language }),
                    }),
                    fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ key: 'ui.accent', value: accent }),
                    }),
                    fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ key: 'ui.texture', value: texture }),
                    }),
                ]);

                if (responses.every(function(resp) { return resp.ok; })) {
                    applyTheme(theme);
                    applyLanguage(language);
                    btn.textContent = 'Saved!';
                    setTimeout(function() {
                        btn.textContent = originalText;
                        btn.disabled = false;
                    }, 1500);
                } else {
                    throw new Error('Failed to save appearance');
                }
            } catch (err) {
                console.error('[Appearance] Save error:', err);
                btn.textContent = 'Error!';
                setTimeout(function() {
                    btn.textContent = originalText;
                    btn.disabled = false;
                }, 1500);
            }
        });
    }

    // --- Theme ---

    function applyTheme(theme) {
        localStorage.setItem('homun-theme', theme);
        document.documentElement.classList.remove('dark');

        if (theme === 'system') {
            var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            if (prefersDark) {
                document.documentElement.classList.add('dark');
            }
        } else if (theme === 'dark') {
            document.documentElement.classList.add('dark');
        }
    }

    function applyLanguage(language) {
        localStorage.setItem('homun-language', language);
        var resolved = language === 'system'
            ? ((navigator.language || 'en').split('-')[0] || 'en')
            : language;
        document.documentElement.lang = resolved;
    }

    // --- Accent color helpers (from shared accent-utils.js) ---
    var applyAccent = window.HomunAccent.applyAccent;
    var deriveAccentFamily = window.HomunAccent.deriveAccentFamily;

    // --- Init ---

    if (themeSelect) {
        applyTheme(themeSelect.value);

        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function() {
            if (themeSelect.value === 'system') {
                applyTheme('system');
            }
        });
    }
    if (languageSelect) {
        applyLanguage(languageSelect.value);
    }

    // Accent picker — presets
    var accentPicker = document.getElementById('accent-picker');
    if (accentPicker) {
        var currentAccent = localStorage.getItem('homun-accent') || '';
        var presetSwatches = accentPicker.querySelectorAll('.accent-swatch[data-accent]');
        presetSwatches.forEach(function(swatch) {
            if (swatch.getAttribute('data-accent') === currentAccent) {
                swatch.classList.add('is-active');
            }
            swatch.addEventListener('click', function() {
                var accent = this.getAttribute('data-accent');
                if (accent !== null) applyAccent(accent);
            });
        });

        // Custom color picker
        var customInput = document.getElementById('accent-custom-input');
        var customLabel = document.querySelector('.accent-custom-label');
        if (customInput) {
            if (currentAccent.startsWith('#')) {
                customInput.value = currentAccent;
                if (customLabel) {
                    customLabel.classList.add('is-active');
                    var preview = customLabel.querySelector('.accent-custom-preview');
                    if (preview) preview.style.background = currentAccent;
                }
                deriveAccentFamily(currentAccent);
            }

            customInput.addEventListener('input', function() {
                applyAccent(this.value);
            });
        }
    }

    // --- Texture picker ---

    var texturePicker = document.getElementById('texture-picker');
    if (texturePicker) {
        var currentTexture = localStorage.getItem('homun-texture') || 'none';
        var textureSwatches = texturePicker.querySelectorAll('.texture-swatch');

        // Set initial active state
        textureSwatches.forEach(function(swatch) {
            var tex = swatch.getAttribute('data-texture');
            swatch.classList.toggle('is-active', tex === currentTexture);

            swatch.addEventListener('click', function() {
                var selected = this.getAttribute('data-texture');
                applyTexture(selected);
            });
        });
    }

    function applyTexture(texture) {
        localStorage.setItem('homun-texture', texture);
        document.documentElement.setAttribute('data-texture', texture);

        // Update .content element class
        var content = document.querySelector('.content');
        if (content) {
            // Remove all texture classes
            var classes = content.className.split(' ').filter(function(c) {
                return !c.startsWith('bg-texture-');
            });
            if (texture !== 'none') {
                classes.push('bg-texture-' + texture);
            }
            content.className = classes.join(' ');
        }

        // Update active state on swatches
        var swatches = document.querySelectorAll('.texture-swatch');
        swatches.forEach(function(s) {
            s.classList.toggle('is-active', s.getAttribute('data-texture') === texture);
        });
    }

    console.log('[Appearance] Page initialized');
})();
