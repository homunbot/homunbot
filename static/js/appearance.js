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

    // --- Accent color helpers ---

    function hexToHSL(hex) {
        var r = parseInt(hex.slice(1,3), 16) / 255;
        var g = parseInt(hex.slice(3,5), 16) / 255;
        var b = parseInt(hex.slice(5,7), 16) / 255;
        var max = Math.max(r, g, b), min = Math.min(r, g, b);
        var h, s, l = (max + min) / 2;
        if (max === min) { h = s = 0; }
        else {
            var d = max - min;
            s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
            if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
            else if (max === g) h = ((b - r) / d + 2) / 6;
            else h = ((r - g) / d + 4) / 6;
        }
        return [Math.round(h * 360), Math.round(s * 100), Math.round(l * 100)];
    }

    function hslToHex(h, s, l) {
        s /= 100; l /= 100;
        var a = s * Math.min(l, 1 - l);
        function f(n) {
            var k = (n + h / 30) % 12;
            var c = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
            return Math.round(255 * c).toString(16).padStart(2, '0');
        }
        return '#' + f(0) + f(8) + f(4);
    }

    function deriveAccentFamily(hex) {
        var hsl = hexToHSL(hex);
        var h = hsl[0], s = hsl[1], l = hsl[2];
        var isDark = document.documentElement.classList.contains('dark');
        var root = document.documentElement.style;

        root.setProperty('--accent', hex);
        root.setProperty('--accent-text', isDark ? hslToHex(h, Math.min(s + 10, 100), Math.min(l + 15, 85)) : hex);
        root.setProperty('--accent-hover', hslToHex(h, s, isDark ? Math.min(l + 8, 80) : Math.max(l - 8, 20)));
        root.setProperty('--accent-active', hslToHex(h, s, isDark ? l : Math.max(l - 14, 15)));
        root.setProperty('--accent-light', hslToHex(h, isDark ? Math.max(s - 30, 10) : Math.min(s + 5, 40), isDark ? 18 : 90));
        root.setProperty('--accent-border', hslToHex(h, isDark ? Math.max(s - 15, 15) : Math.min(s, 35), isDark ? 30 : 75));
        root.setProperty('--focus-ring', hslToHex(h, Math.min(s + 5, 60), isDark ? Math.min(l + 10, 70) : Math.min(l + 10, 55)));
        root.setProperty('--selection-bg', hslToHex(h, isDark ? 20 : 25, isDark ? 22 : 82));
        root.setProperty('--chart-primary', hex);
        // Nav bar uses accent color as background
        root.setProperty('--nav-bg', hex);
        // Contrast text for accent backgrounds (user bubbles, etc.)
        root.setProperty('--accent-contrast', l > 55 ? '#1a1a1a' : '#ffffff');
    }

    function clearCustomAccent() {
        var props = ['--accent', '--accent-text', '--accent-hover', '--accent-active',
                     '--accent-light', '--accent-border', '--focus-ring', '--selection-bg', '--chart-primary', '--nav-bg', '--accent-contrast'];
        props.forEach(function(p) { document.documentElement.style.removeProperty(p); });
    }

    function applyAccent(accent) {
        localStorage.setItem('homun-accent', accent);
        clearCustomAccent();

        if (accent.startsWith('#')) {
            document.documentElement.removeAttribute('data-accent');
            deriveAccentFamily(accent);
            localStorage.setItem('homun-accent-custom', accent);
        } else if (accent === '' || accent === 'moss') {
            // Blue (default) or Moss — remove data-accent, CSS :root handles it
            document.documentElement.removeAttribute('data-accent');
            if (accent === 'moss') {
                document.documentElement.setAttribute('data-accent', 'moss');
            }
        } else {
            document.documentElement.setAttribute('data-accent', accent);
        }

        var swatches = document.querySelectorAll('.accent-swatch');
        var customLabel = document.querySelector('.accent-custom-label');
        swatches.forEach(function(s) {
            if (s === customLabel) {
                s.classList.toggle('is-active', accent.startsWith('#'));
            } else {
                s.classList.toggle('is-active', s.getAttribute('data-accent') === accent);
            }
        });

        if (customLabel && accent.startsWith('#')) {
            var preview = customLabel.querySelector('.accent-custom-preview');
            if (preview) preview.style.background = accent;
        }
    }

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
