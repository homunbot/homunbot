// Homun — Shared accent color utilities
// Used by appearance.js and onboarding.js

(function () {
    'use strict';

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
        root.setProperty('--nav-bg', hex);
        root.setProperty('--accent-contrast', l > 55 ? '#1a1a1a' : '#ffffff');
    }

    function clearCustomAccent() {
        var props = ['--accent', '--accent-text', '--accent-hover', '--accent-active',
                     '--accent-light', '--accent-border', '--focus-ring', '--selection-bg',
                     '--chart-primary', '--nav-bg', '--accent-contrast'];
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

    function applyTheme(theme) {
        localStorage.setItem('homun-theme', theme);
        var isDark = theme === 'dark' ||
            (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
        document.documentElement.classList.toggle('dark', isDark);
        // Re-derive accent if custom
        var accent = localStorage.getItem('homun-accent') || '';
        if (accent.startsWith('#')) {
            deriveAccentFamily(accent);
        }
    }

    window.HomunAccent = {
        hexToHSL: hexToHSL,
        hslToHex: hslToHex,
        deriveAccentFamily: deriveAccentFamily,
        clearCustomAccent: clearCustomAccent,
        applyAccent: applyAccent,
        applyTheme: applyTheme
    };
})();
