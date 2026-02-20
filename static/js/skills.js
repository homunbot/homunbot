// Homun — Skills Manager
// Search ClawHub & GitHub, install, remove skills

(function() {
    'use strict';

    // --- DOM refs ---
    var searchInput = document.getElementById('skill-search-input');
    var searchSpinner = document.getElementById('search-spinner');
    var searchSection = document.getElementById('search-section');
    var searchGrid = document.getElementById('search-grid');
    var searchCount = document.getElementById('search-count');
    var installedGrid = document.getElementById('installed-grid');
    var installedCount = document.getElementById('installed-count');
    var toastEl = document.getElementById('skill-toast');

    // Track installed skill names for cross-referencing search results
    var installedNames = new Set();
    document.querySelectorAll('#installed-grid .skill-card').forEach(function(card) {
        var name = card.getAttribute('data-skill-name');
        if (name) installedNames.add(name);
    });

    // --- Debounce helper ---
    var debounceTimer = null;
    function debounce(fn, ms) {
        return function() {
            var args = arguments;
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(function() { fn.apply(null, args); }, ms);
        };
    }

    // --- Toast ---
    var toastTimeout = null;
    function showToast(message, type) {
        if (!toastEl) return;
        toastEl.textContent = message;
        toastEl.className = 'skill-toast skill-toast--' + (type || 'success');
        toastEl.style.display = 'block';
        clearTimeout(toastTimeout);
        toastTimeout = setTimeout(function() {
            toastEl.style.display = 'none';
        }, 3000);
    }

    // --- Format numbers ---
    function formatNum(n) {
        if (n >= 1000) return (n / 1000).toFixed(1) + 'k';
        return String(n);
    }

    // --- Safe DOM helpers ---
    function el(tag, className, textContent) {
        var e = document.createElement(tag);
        if (className) e.className = className;
        if (textContent) e.textContent = textContent;
        return e;
    }

    function svgStar() {
        var s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        s.setAttribute('viewBox', '0 0 16 16');
        s.setAttribute('fill', 'currentColor');
        var p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        p.setAttribute('d', 'M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z');
        s.appendChild(p);
        return s;
    }

    function svgDownload() {
        var s = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        s.setAttribute('viewBox', '0 0 16 16');
        s.setAttribute('fill', 'currentColor');
        var p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        p.setAttribute('d', 'M2.75 14A1.75 1.75 0 0 1 1 12.25v-2.5a.75.75 0 0 1 1.5 0v2.5c0 .138.112.25.25.25h10.5a.25.25 0 0 0 .25-.25v-2.5a.75.75 0 0 1 1.5 0v2.5A1.75 1.75 0 0 1 13.25 14zM7.25 7.689V2a.75.75 0 0 1 1.5 0v5.689l1.97-1.969a.749.749 0 1 1 1.06 1.06l-3.25 3.25a.749.749 0 0 1-1.06 0L4.22 6.78a.749.749 0 1 1 1.06-1.06z');
        s.appendChild(p);
        return s;
    }

    // --- Search ---
    var debouncedSearch = debounce(doSearch, 300);

    if (searchInput) {
        searchInput.addEventListener('input', function() {
            var q = searchInput.value.trim();
            if (q.length < 2) {
                hideResults();
                return;
            }
            // If it looks like a direct slug (contains /), don't auto-search
            if (q.indexOf('/') !== -1) return;
            debouncedSearch(q);
        });

        searchInput.addEventListener('keydown', function(e) {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            var q = searchInput.value.trim();
            if (!q) return;

            // Direct install if it contains /
            if (q.indexOf('/') !== -1) {
                directInstall(q);
            } else if (q.length >= 2) {
                doSearch(q);
            }
        });
    }

    function hideResults() {
        if (searchSection) searchSection.style.display = 'none';
        if (searchGrid) searchGrid.textContent = '';
    }

    async function doSearch(query) {
        if (searchSpinner) searchSpinner.style.display = 'block';

        try {
            var res = await fetch('/api/v1/skills/search?q=' + encodeURIComponent(query));
            var results = await res.json();
            renderSearchResults(results);
        } catch (err) {
            showToast('Search failed: ' + err.message, 'error');
        } finally {
            if (searchSpinner) searchSpinner.style.display = 'none';
        }
    }

    function renderSearchResults(results) {
        if (!searchGrid || !searchSection) return;

        searchGrid.textContent = '';
        searchSection.style.display = 'block';

        if (searchCount) {
            searchCount.textContent = results.length + ' found';
        }

        if (results.length === 0) {
            var empty = el('div', 'empty-state');
            var p = el('p', null, 'No skills found. Try a different search term.');
            empty.appendChild(p);
            searchGrid.appendChild(empty);
            return;
        }

        results.forEach(function(skill) {
            var card = buildSearchCard(skill);
            searchGrid.appendChild(card);
        });
    }

    function buildSearchCard(skill) {
        var card = el('div', 'skill-card');

        // Determine display name: strip "clawhub:" prefix for display
        var displayName = skill.name;
        if (displayName.indexOf('clawhub:') === 0) {
            displayName = displayName.substring(8);
        }

        // Check if already installed — match by the last segment (skill name)
        var skillBaseName = displayName.split('/').pop() || displayName;
        var isInstalled = installedNames.has(skillBaseName);

        // Header
        var header = el('div', 'skill-card-header');
        var nameEl = el('div', 'skill-name', displayName);
        header.appendChild(nameEl);

        var badge = el('span', 'skill-source-badge skill-source-badge--' + skill.source);
        badge.textContent = skill.source === 'clawhub' ? 'ClawHub' : 'GitHub';
        header.appendChild(badge);
        card.appendChild(header);

        // Description
        var desc = el('div', 'skill-desc', skill.description || 'No description');
        card.appendChild(desc);

        // Meta row (stats)
        if (skill.stars > 0 || skill.downloads > 0) {
            var meta = el('div', 'skill-meta');

            if (skill.stars > 0) {
                var starSpan = el('span', 'skill-stat');
                starSpan.appendChild(svgStar());
                starSpan.appendChild(document.createTextNode(' ' + formatNum(skill.stars)));
                meta.appendChild(starSpan);
            }
            if (skill.downloads > 0) {
                var dlSpan = el('span', 'skill-stat');
                dlSpan.appendChild(svgDownload());
                dlSpan.appendChild(document.createTextNode(' ' + formatNum(skill.downloads)));
                meta.appendChild(dlSpan);
            }

            card.appendChild(meta);
        }

        // Actions
        var actions = el('div', 'skill-card-actions');

        if (isInstalled) {
            var installedBtn = el('button', 'btn btn-sm btn-installed', 'Installed');
            actions.appendChild(installedBtn);
        } else {
            var installBtn = el('button', 'btn btn-sm btn-primary', 'Install');
            installBtn.addEventListener('click', function() {
                installSkill(skill.name, installBtn, card);
            });
            actions.appendChild(installBtn);
        }

        card.appendChild(actions);
        return card;
    }

    // --- Install ---
    async function installSkill(source, btn, card) {
        if (btn) {
            btn.textContent = 'Installing...';
            btn.disabled = true;
        }
        if (card) card.classList.add('skill-card--installing');

        try {
            var res = await fetch('/api/v1/skills/install', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ source: source }),
            });
            var data = await res.json();

            if (data.ok) {
                showToast('Installed: ' + data.name, 'success');
                installedNames.add(data.name);

                // Update button to "Installed"
                if (btn) {
                    btn.textContent = 'Installed';
                    btn.className = 'btn btn-sm btn-installed';
                    btn.disabled = true;
                }
                if (card) card.classList.remove('skill-card--installing');

                // Refresh installed section
                refreshInstalled();
            } else {
                showToast(data.message || 'Install failed', 'error');
                if (btn) {
                    btn.textContent = 'Install';
                    btn.disabled = false;
                }
                if (card) card.classList.remove('skill-card--installing');
            }
        } catch (err) {
            showToast('Install failed: ' + err.message, 'error');
            if (btn) {
                btn.textContent = 'Install';
                btn.disabled = false;
            }
            if (card) card.classList.remove('skill-card--installing');
        }
    }

    // --- Direct install (from search bar with owner/repo) ---
    async function directInstall(slug) {
        if (searchSpinner) searchSpinner.style.display = 'block';
        searchInput.disabled = true;

        try {
            var res = await fetch('/api/v1/skills/install', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ source: slug }),
            });
            var data = await res.json();

            if (data.ok) {
                showToast('Installed: ' + data.name, 'success');
                installedNames.add(data.name);
                searchInput.value = '';
                refreshInstalled();
            } else {
                showToast(data.message || 'Install failed', 'error');
            }
        } catch (err) {
            showToast('Install failed: ' + err.message, 'error');
        } finally {
            if (searchSpinner) searchSpinner.style.display = 'none';
            searchInput.disabled = false;
            searchInput.focus();
        }
    }

    // --- Remove ---
    function setupRemoveButtons() {
        document.querySelectorAll('.skill-remove-btn').forEach(function(btn) {
            btn.addEventListener('click', function(e) {
                e.stopPropagation();
                var skillName = btn.getAttribute('data-skill');
                if (!skillName) return;
                if (!confirm('Remove skill "' + skillName + '"?')) return;
                removeSkill(skillName, btn);
            });
        });
    }

    async function removeSkill(name, btn) {
        if (btn) {
            btn.textContent = 'Removing...';
            btn.disabled = true;
        }

        try {
            var res = await fetch('/api/v1/skills/' + encodeURIComponent(name), {
                method: 'DELETE',
            });
            var data = await res.json();

            if (data.ok) {
                showToast(data.message, 'success');
                installedNames.delete(name);

                // Remove card with fade
                var card = document.querySelector('.skill-card[data-skill-name="' + CSS.escape(name) + '"]');
                if (card) {
                    card.style.transition = 'opacity 0.3s ease';
                    card.style.opacity = '0';
                    setTimeout(function() { card.remove(); updateInstalledCount(); }, 300);
                }
            } else {
                showToast(data.message || 'Remove failed', 'error');
                if (btn) {
                    btn.textContent = 'Remove';
                    btn.disabled = false;
                }
            }
        } catch (err) {
            showToast('Remove failed: ' + err.message, 'error');
            if (btn) {
                btn.textContent = 'Remove';
                btn.disabled = false;
            }
        }
    }

    // --- Refresh installed section from API ---
    async function refreshInstalled() {
        try {
            var res = await fetch('/api/v1/skills');
            var skills = await res.json();

            installedNames.clear();
            skills.forEach(function(s) { installedNames.add(s.name); });

            if (!installedGrid) return;
            installedGrid.textContent = '';

            if (skills.length === 0) {
                var empty = el('div', 'empty-state');
                empty.id = 'installed-empty';
                var icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
                icon.setAttribute('class', 'empty-state-icon');
                icon.setAttribute('viewBox', '0 0 24 24');
                icon.setAttribute('fill', 'none');
                icon.setAttribute('stroke', 'currentColor');
                icon.setAttribute('stroke-width', '1.5');
                var path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                path.setAttribute('d', 'M12 2L15 8.5 22 9.5 17 14.5 18 22 12 19 6 22 7 14.5 2 9.5 9 8.5z');
                icon.appendChild(path);
                empty.appendChild(icon);
                empty.appendChild(el('p', null, 'No skills installed yet.'));
                var hint = el('p');
                hint.textContent = 'Search ClawHub or enter owner/repo to install.';
                empty.appendChild(hint);
                installedGrid.appendChild(empty);
            } else {
                skills.forEach(function(s) {
                    var card = buildInstalledCard(s);
                    installedGrid.appendChild(card);
                });
            }

            updateInstalledCount();
        } catch (err) {
            // Silent — installed list will be stale but functional
        }
    }

    function buildInstalledCard(s) {
        var card = el('div', 'skill-card');
        card.setAttribute('data-skill-name', s.name);

        var sourceClass = s.source === 'clawhub' ? 'clawhub' : 'github';
        var sourceLabel = s.source === 'clawhub' ? 'ClawHub' : 'GitHub';

        // Header
        var header = el('div', 'skill-card-header');
        header.appendChild(el('div', 'skill-name', s.name));
        header.appendChild(el('span', 'skill-source-badge skill-source-badge--' + sourceClass, sourceLabel));
        card.appendChild(header);

        // Description
        card.appendChild(el('div', 'skill-desc', s.description));

        // Footer
        var footer = el('div', 'skill-card-footer');
        footer.appendChild(el('span', 'skill-path', s.path));

        var removeBtn = el('button', 'btn btn-sm btn-danger skill-remove-btn', 'Remove');
        removeBtn.setAttribute('data-skill', s.name);
        removeBtn.addEventListener('click', function(e) {
            e.stopPropagation();
            if (!confirm('Remove skill "' + s.name + '"?')) return;
            removeSkill(s.name, removeBtn);
        });
        footer.appendChild(removeBtn);

        card.appendChild(footer);
        return card;
    }

    function updateInstalledCount() {
        if (!installedCount) return;
        var count = document.querySelectorAll('#installed-grid .skill-card[data-skill-name]').length;
        installedCount.textContent = count + ' installed';
    }

    // --- Init ---
    setupRemoveButtons();

})();
