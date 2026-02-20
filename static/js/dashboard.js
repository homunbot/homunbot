// Homun — Dashboard inline editing + live status

// ─── Inline Stat Card Editing ───
document.querySelectorAll('.stat-card[data-editable]').forEach(card => {
    card.addEventListener('click', (e) => {
        if (card.classList.contains('editing') || e.target.closest('.inline-edit')) return;
        card.classList.add('editing');
        const input = card.querySelector('.inline-input');
        if (input) {
            input.focus();
            input.select();
        }
    });
});

// Save handler for inline edits
document.querySelectorAll('.inline-edit').forEach(form => {
    const card = form.closest('.stat-card');
    const key = card?.dataset.key;
    const input = form.querySelector('.inline-input');
    const saveBtn = form.querySelector('.btn-save');
    const cancelBtn = form.querySelector('.btn-cancel');

    async function save() {
        if (!key || !input) return;
        const value = input.value.trim();
        if (!value) return cancel();

        try {
            await fetch('/api/v1/config', {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ key, value }),
            });
            // Update displayed value
            const valEl = card.querySelector('.stat-value');
            if (valEl) valEl.textContent = value;
            card.classList.remove('editing');
            showToast('Saved. Restart to apply.', 'success');
        } catch (err) {
            showToast('Failed to save', 'error');
        }
    }

    function cancel() {
        card.classList.remove('editing');
        // Restore original value
        const valEl = card.querySelector('.stat-value');
        if (valEl && input) input.value = valEl.textContent;
    }

    if (saveBtn) saveBtn.addEventListener('click', (e) => { e.stopPropagation(); save(); });
    if (cancelBtn) cancelBtn.addEventListener('click', (e) => { e.stopPropagation(); cancel(); });

    if (input) {
        input.addEventListener('keydown', (e) => {
            e.stopPropagation();
            if (e.key === 'Enter') save();
            if (e.key === 'Escape') cancel();
        });
        input.addEventListener('click', (e) => e.stopPropagation());
    }
});

// ─── Toast notifications ───
function showToast(message, type = 'success') {
    const existing = document.querySelector('.toast');
    if (existing) existing.remove();

    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);

    setTimeout(() => {
        toast.classList.add('toast-out');
        setTimeout(() => toast.remove(), 300);
    }, 2500);
}

// ─── Live uptime counter ───
const uptimeEl = document.querySelector('[data-live-uptime]');
if (uptimeEl) {
    const startSecs = parseInt(uptimeEl.dataset.liveUptime, 10);
    const startedAt = Date.now() - (startSecs * 1000);

    function updateUptime() {
        const secs = Math.floor((Date.now() - startedAt) / 1000);
        if (secs < 60) uptimeEl.textContent = secs + 's';
        else if (secs < 3600) uptimeEl.textContent = Math.floor(secs/60) + 'm ' + (secs%60) + 's';
        else if (secs < 86400) uptimeEl.textContent = Math.floor(secs/3600) + 'h ' + Math.floor((secs%3600)/60) + 'm';
        else uptimeEl.textContent = Math.floor(secs/86400) + 'd ' + Math.floor((secs%86400)/3600) + 'h';
    }

    updateUptime();
    setInterval(updateUptime, 1000);
}
