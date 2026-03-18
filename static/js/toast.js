// Homun — Global toast notification utility
// Unified replacement for 6 different per-page toast implementations.
// Position: fixed bottom-right (24px), consistent across all pages.

window.showToast = function(message, type, duration) {
    type = type || 'success';
    duration = duration || 2500;

    var existing = document.querySelector('.hm-toast');
    if (existing) existing.remove();

    var toast = document.createElement('div');
    toast.className = 'hm-toast hm-toast--' + type;
    toast.textContent = message;
    document.body.appendChild(toast);

    requestAnimationFrame(function() {
        toast.classList.add('hm-toast--visible');
    });

    setTimeout(function() {
        toast.classList.remove('hm-toast--visible');
        setTimeout(function() { toast.remove(); }, 200);
    }, duration);
};

// Show an inline error state with optional retry button inside a container.
// Uses safe DOM APIs (createElement/textContent) — no innerHTML.
window.showErrorState = function(containerId, message, retryFn) {
    var el = document.getElementById(containerId);
    if (!el) return;

    // Remove existing error state if any
    var prev = el.querySelector('.hm-error-state');
    if (prev) prev.remove();

    var wrapper = document.createElement('div');
    wrapper.className = 'hm-error-state';

    // Warning icon (SVG)
    var ns = 'http://www.w3.org/2000/svg';
    var svg = document.createElementNS(ns, 'svg');
    svg.setAttribute('viewBox', '0 0 24 24');
    svg.setAttribute('fill', 'none');
    svg.setAttribute('stroke', 'currentColor');
    svg.setAttribute('stroke-width', '1.5');
    var circle = document.createElementNS(ns, 'circle');
    circle.setAttribute('cx', '12'); circle.setAttribute('cy', '12'); circle.setAttribute('r', '10');
    var line = document.createElementNS(ns, 'line');
    line.setAttribute('x1', '12'); line.setAttribute('y1', '8');
    line.setAttribute('x2', '12'); line.setAttribute('y2', '12');
    var dot = document.createElementNS(ns, 'circle');
    dot.setAttribute('cx', '12'); dot.setAttribute('cy', '16'); dot.setAttribute('r', '0.5');
    dot.setAttribute('fill', 'currentColor');
    svg.appendChild(circle); svg.appendChild(line); svg.appendChild(dot);

    var p = document.createElement('p');
    p.textContent = message;

    wrapper.appendChild(svg);
    wrapper.appendChild(p);

    if (typeof retryFn === 'function') {
        var btn = document.createElement('button');
        btn.className = 'hm-retry-btn';
        btn.textContent = 'Retry';
        btn.addEventListener('click', retryFn);
        wrapper.appendChild(btn);
    }

    el.appendChild(wrapper);
};

// Show a persistent progress toast with spinner. Returns nothing — call hideProgressToast() to dismiss.
window.showProgressToast = function(message) {
    var existing = document.querySelector('.hm-toast--progress');
    if (existing) existing.remove();

    var toast = document.createElement('div');
    toast.className = 'hm-toast hm-toast--progress';

    var spinner = document.createElement('span');
    spinner.className = 'hm-spinner hm-spinner--sm';
    toast.appendChild(spinner);

    var text = document.createElement('span');
    text.textContent = message;
    toast.appendChild(text);

    document.body.appendChild(toast);
    requestAnimationFrame(function() { toast.classList.add('hm-toast--visible'); });
};

// Update the text of an existing progress toast (no-op if none visible).
window.updateProgressToast = function(message) {
    var toast = document.querySelector('.hm-toast--progress');
    if (!toast) return;
    var text = toast.querySelector('span:not(.hm-spinner)');
    if (text) text.textContent = message;
};

// Dismiss the progress toast.
window.hideProgressToast = function() {
    var toast = document.querySelector('.hm-toast--progress');
    if (!toast) return;
    toast.classList.remove('hm-toast--visible');
    setTimeout(function() { toast.remove(); }, 200);
};

// Show an inline progress indicator inside a container (spinner + text).
window.showProgress = function(containerId, message) {
    var el = document.getElementById(containerId);
    if (!el) return;
    el.style.display = 'block';
    el.textContent = '';
    el.className = (el.className.replace(/\bhm-progress\b/, '') + ' hm-progress').trim();
    var spinner = document.createElement('span');
    spinner.className = 'hm-spinner hm-spinner--sm';
    el.appendChild(spinner);
    var text = document.createElement('span');
    text.textContent = message;
    el.appendChild(text);
};

// Clear inline progress — reset to plain container.
window.hideProgress = function(containerId) {
    var el = document.getElementById(containerId);
    if (!el) return;
    el.className = el.className.replace(/\bhm-progress\b/, '').trim();
    el.textContent = '';
};

window.clearErrorState = function(containerId) {
    var el = document.getElementById(containerId);
    if (!el) return;
    var err = el.querySelector('.hm-error-state');
    if (err) err.remove();
};
