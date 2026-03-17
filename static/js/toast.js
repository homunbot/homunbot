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
