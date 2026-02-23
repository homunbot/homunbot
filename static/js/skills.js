// Homun — Skills Manager
// Search ClawHub, GitHub & Open Skills, install, remove, detail modal

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
    var catalogStatsEl = document.getElementById('catalog-stats');

    // Modal refs
    var modalOverlay = document.getElementById('skill-modal-overlay');
    var modalTitle = document.getElementById('modal-title');
    var modalSubtitle = document.getElementById('modal-subtitle');
    var modalClose = document.getElementById('modal-close');
    var modalMeta = document.getElementById('modal-meta');
    var modalContent = document.getElementById('modal-content');
    var modalFooter = document.getElementById('modal-footer');

    // Track installed skill names for cross-referencing search results
    var installedNames = new Set();
    document.querySelectorAll('#installed-grid .skill-card').forEach(function(card) {
        var name = card.getAttribute('data-skill-name');
        if (name) installedNames.add(name);
    });

    // --- Catalog cache check ---
    var catalogBanner = document.getElementById('catalog-banner');
    var catalogBar = document.getElementById('catalog-bar');
    var catalogTitle = document.getElementById('catalog-banner-title');
    var catalogDetail = document.getElementById('catalog-banner-detail');

    function checkCatalog() {
        fetch('/api/v1/skills/catalog/status')
            .then(function(r) { return r.json(); })
            .then(function(data) {
                if (!data.cached || data.stale) {
                    catalogBanner.style.display = 'block';
                    catalogTitle.textContent = data.cached
                        ? 'Updating skill catalog...'
                        : 'Downloading skill catalog...';
                    catalogDetail.textContent = data.cached
                        ? 'Refreshing ' + data.skill_count + ' skills from ClawHub.'
                        : 'First time setup — indexing 9,000+ skills from ClawHub.';
                    var progress = 0;
                    var barInterval = setInterval(function() {
                        progress = Math.min(progress + 2, 90);
                        catalogBar.style.width = progress + '%';
                    }, 1000);
                    fetch('/api/v1/skills/catalog/refresh', { method: 'POST' })
                        .then(function(r) { return r.json(); })
                        .then(function(result) {
                            clearInterval(barInterval);
                            catalogBar.style.width = '100%';
                            catalogBanner.classList.add('catalog-banner--done');
                            catalogTitle.textContent = 'Catalog ready';
                            catalogDetail.textContent = result.skill_count + ' skills indexed.';
                            setTimeout(function() {
                                catalogBanner.style.display = 'none';
                                catalogBanner.classList.remove('catalog-banner--done');
                            }, 2000);
                            // Refresh catalog stats after refresh
                            fetchCatalogStats();
                        })
                        .catch(function() {
                            clearInterval(barInterval);
                            catalogTitle.textContent = 'Catalog update failed';
                            catalogDetail.textContent = 'Search may be slower. Try again later.';
                            setTimeout(function() { catalogBanner.style.display = 'none'; }, 4000);
                        });
                }
            })
            .catch(function() { /* silently ignore */ });
    }
    checkCatalog();

    // --- Catalog source stats (under search bar) ---
    function fetchCatalogStats() {
        if (!catalogStatsEl) return;
        fetch('/api/v1/skills/catalog/counts')
            .then(function(r) { return r.json(); })
            .then(function(data) {
                catalogStatsEl.textContent = '';
                var parts = [];
                if (data.clawhub > 0) parts.push({ source: 'clawhub', count: data.clawhub });
                if (data.openskills > 0) parts.push({ source: 'openskills', count: data.openskills });
                // GitHub: live search (no cached catalog)
                parts.push({ source: 'github', count: -1 });

                var prefix = el('span', null, 'Search across ');
                catalogStatsEl.appendChild(prefix);

                parts.forEach(function(p, i) {
                    if (i > 0) catalogStatsEl.appendChild(document.createTextNode('  ·  '));
                    var stat = el('span', 'catalog-stat');
                    var dot = el('span', 'catalog-stat-dot catalog-stat-dot--' + p.source);
                    stat.appendChild(dot);
                    var label = p.count > 0
                        ? formatNum(p.count) + ' ' + sourceLabel(p.source)
                        : sourceLabel(p.source) + (p.count < 0 ? ' (live)' : '');
                    stat.appendChild(document.createTextNode(label));
                    catalogStatsEl.appendChild(stat);
                });
            })
            .catch(function() { /* silently ignore */ });
    }
    fetchCatalogStats();

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

    /** Map source id to display label */
    function sourceLabel(src) {
        if (src === 'clawhub') return 'ClawHub';
        if (src === 'openskills') return 'Open Skills';
        return 'GitHub';
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
            if (q.indexOf('/') !== -1) return;
            debouncedSearch(q);
        });

        searchInput.addEventListener('keydown', function(e) {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            var q = searchInput.value.trim();
            if (!q) return;
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
        if (searchCount) searchCount.textContent = results.length + ' found';

        if (results.length === 0) {
            var empty = el('div', 'empty-state');
            empty.appendChild(el('p', null, 'No skills found. Try a different search term.'));
            searchGrid.appendChild(empty);
            return;
        }

        results.forEach(function(skill) {
            searchGrid.appendChild(buildSearchCard(skill));
        });
    }

    function buildSearchCard(skill) {
        var card = el('div', 'skill-card');

        var displayName = skill.name;
        if (displayName.indexOf('clawhub:') === 0) displayName = displayName.substring(8);
        if (displayName.indexOf('openskills:') === 0) displayName = displayName.substring(11);

        var skillBaseName = displayName.split('/').pop() || displayName;
        var isInstalled = installedNames.has(skillBaseName);

        // Header
        var header = el('div', 'skill-card-header');
        header.appendChild(el('div', 'skill-name', displayName));
        var badge = el('span', 'skill-source-badge skill-source-badge--' + skill.source);
        badge.textContent = sourceLabel(skill.source);
        header.appendChild(badge);
        card.appendChild(header);

        // Description
        card.appendChild(el('div', 'skill-desc', skill.description || 'No description'));

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
            actions.appendChild(el('button', 'btn btn-sm btn-installed', 'Installed'));
        } else {
            var installBtn = el('button', 'btn btn-sm btn-primary', 'Install');
            installBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                installSkill(skill.name, installBtn, card);
            });
            actions.appendChild(installBtn);
        }
        card.appendChild(actions);

        // Click card to show detail
        card.addEventListener('click', function() {
            if (isInstalled) {
                openInstalledDetail(skillBaseName);
            } else {
                openSearchDetail(skill, displayName);
            }
        });

        return card;
    }

    // --- Install ---
    async function installSkill(source, btn, card) {
        if (btn) { btn.textContent = 'Installing...'; btn.disabled = true; }
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
                if (btn) { btn.textContent = 'Installed'; btn.className = 'btn btn-sm btn-installed'; btn.disabled = true; }
                if (card) card.classList.remove('skill-card--installing');
                refreshInstalled();
            } else {
                showToast(data.message || 'Install failed', 'error');
                if (btn) { btn.textContent = 'Install'; btn.disabled = false; }
                if (card) card.classList.remove('skill-card--installing');
            }
        } catch (err) {
            showToast('Install failed: ' + err.message, 'error');
            if (btn) { btn.textContent = 'Install'; btn.disabled = false; }
            if (card) card.classList.remove('skill-card--installing');
        }
    }

    // --- Direct install ---
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
        if (btn) { btn.textContent = 'Removing...'; btn.disabled = true; }
        try {
            var res = await fetch('/api/v1/skills/' + encodeURIComponent(name), { method: 'DELETE' });
            var data = await res.json();
            if (data.ok) {
                showToast(data.message, 'success');
                installedNames.delete(name);
                var card = document.querySelector('.skill-card[data-skill-name="' + CSS.escape(name) + '"]');
                if (card) {
                    card.style.transition = 'opacity 0.3s ease';
                    card.style.opacity = '0';
                    setTimeout(function() { card.remove(); updateInstalledCount(); }, 300);
                }
                // Close modal if it was showing this skill
                closeModal();
            } else {
                showToast(data.message || 'Remove failed', 'error');
                if (btn) { btn.textContent = 'Remove'; btn.disabled = false; }
            }
        } catch (err) {
            showToast('Remove failed: ' + err.message, 'error');
            if (btn) { btn.textContent = 'Remove'; btn.disabled = false; }
        }
    }

    // --- Refresh installed section ---
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
                    installedGrid.appendChild(buildInstalledCard(s));
                });
            }
            updateInstalledCount();
        } catch (err) {
            // Silent
        }
    }

    function buildInstalledCard(s) {
        var card = el('div', 'skill-card');
        card.setAttribute('data-skill-name', s.name);
        card.setAttribute('data-skill-source', s.source || 'github');

        // Header
        var header = el('div', 'skill-card-header');
        header.appendChild(el('div', 'skill-name', s.name));
        header.appendChild(el('span', 'skill-source-badge skill-source-badge--' + s.source, sourceLabel(s.source)));
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

        // Click to open detail modal
        card.addEventListener('click', function() {
            openInstalledDetail(s.name);
        });

        return card;
    }

    function updateInstalledCount() {
        if (!installedCount) return;
        var cards = document.querySelectorAll('#installed-grid .skill-card[data-skill-name]');
        var count = cards.length;
        installedCount.textContent = count + ' installed';

        // Rebuild source counter chips
        var counts = { clawhub: 0, github: 0, openskills: 0 };
        cards.forEach(function(card) {
            var src = card.getAttribute('data-skill-source') || 'github';
            if (counts[src] !== undefined) counts[src]++;
        });

        var chipsEl = document.getElementById('source-chips');
        if (!chipsEl) {
            chipsEl = el('div', 'skill-source-chips');
            chipsEl.id = 'source-chips';
            var titleGroup = installedCount.parentElement;
            if (titleGroup) titleGroup.appendChild(chipsEl);
        }
        chipsEl.textContent = '';
        var sources = ['clawhub', 'github', 'openskills'];
        sources.forEach(function(src) {
            if (counts[src] > 0) {
                chipsEl.appendChild(el('span', 'skill-source-chip skill-source-chip--' + src,
                    counts[src] + ' ' + sourceLabel(src)));
            }
        });
    }

    // --- Detail modal ---

    async function openInstalledDetail(name) {
        if (!modalOverlay) return;
        modalTitle.textContent = name;
        modalSubtitle.textContent = 'Loading...';
        modalMeta.textContent = '';
        modalContent.textContent = '';
        modalFooter.textContent = '';
        modalOverlay.classList.add('active');

        try {
            var res = await fetch('/api/v1/skills/' + encodeURIComponent(name));
            if (!res.ok) throw new Error('Not found');
            var detail = await res.json();

            // Title
            modalTitle.textContent = detail.name;

            // Subtitle: description (first line)
            var shortDesc = detail.description || 'No description';
            modalSubtitle.textContent = shortDesc;

            // Meta bar: source badge + path
            modalMeta.textContent = '';
            var srcBadge = el('span', 'skill-source-badge skill-source-badge--' + detail.source, sourceLabel(detail.source));
            modalMeta.appendChild(srcBadge);
            var pathItem = el('span', 'skill-modal-meta-item');
            pathItem.appendChild(el('code', null, detail.path));
            modalMeta.appendChild(pathItem);

            // Content: HTML rendered server-side via pulldown-cmark
            modalContent.innerHTML = detail.content_html;

            // Footer: scripts list + remove button
            modalFooter.textContent = '';
            if (detail.scripts && detail.scripts.length > 0) {
                var scriptsEl = el('span', 'skill-modal-scripts', 'Scripts: ');
                detail.scripts.forEach(function(s, i) {
                    if (i > 0) scriptsEl.appendChild(document.createTextNode(', '));
                    scriptsEl.appendChild(el('code', null, s));
                });
                modalFooter.appendChild(scriptsEl);
            } else {
                modalFooter.appendChild(el('span', 'skill-modal-scripts', 'Instruction-only skill (no scripts)'));
            }

            var removeBtn = el('button', 'btn btn-sm btn-danger', 'Remove');
            removeBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                if (!confirm('Remove skill "' + detail.name + '"?')) return;
                removeSkill(detail.name, removeBtn);
            });
            modalFooter.appendChild(removeBtn);

        } catch (err) {
            modalSubtitle.textContent = 'Failed to load skill details';
            modalContent.textContent = err.message;
        }
    }

    /** Open detail modal for a search result (not installed yet) */
    function openSearchDetail(skill, displayName) {
        if (!modalOverlay) return;

        modalTitle.textContent = displayName;
        modalSubtitle.textContent = '';

        // Meta bar: source badge + stats + slug
        modalMeta.textContent = '';
        modalMeta.appendChild(el('span', 'skill-source-badge skill-source-badge--' + skill.source, sourceLabel(skill.source)));
        if (skill.stars > 0) {
            var starItem = el('span', 'skill-modal-meta-item');
            starItem.appendChild(svgStar());
            starItem.appendChild(document.createTextNode(' ' + formatNum(skill.stars)));
            modalMeta.appendChild(starItem);
        }
        if (skill.downloads > 0) {
            var dlItem = el('span', 'skill-modal-meta-item');
            dlItem.appendChild(svgDownload());
            dlItem.appendChild(document.createTextNode(' ' + formatNum(skill.downloads)));
            modalMeta.appendChild(dlItem);
        }
        var sourceItem = el('span', 'skill-modal-meta-item');
        sourceItem.appendChild(el('code', null, skill.name));
        modalMeta.appendChild(sourceItem);

        // Content: description only (no SKILL.md available pre-install)
        modalContent.innerHTML = '';
        modalContent.appendChild(el('p', null, skill.description || 'No description available.'));

        // Footer: source label + install button
        modalFooter.textContent = '';
        modalFooter.appendChild(el('span', 'skill-modal-scripts', sourceLabel(skill.source) + ' skill'));

        var skillBaseName = displayName.split('/').pop() || displayName;
        var isInstalled = installedNames.has(skillBaseName);

        if (isInstalled) {
            modalFooter.appendChild(el('button', 'btn btn-sm btn-installed', 'Installed'));
        } else {
            var installBtn = el('button', 'btn btn-sm btn-primary', 'Install');
            installBtn.addEventListener('click', function(e) {
                e.stopPropagation();
                installSkill(skill.name, installBtn, null);
            });
            modalFooter.appendChild(installBtn);
        }

        modalOverlay.classList.add('active');
    }

    function closeModal() {
        if (modalOverlay) modalOverlay.classList.remove('active');
    }

    // Close modal on X button, overlay click, Escape
    if (modalClose) modalClose.addEventListener('click', closeModal);
    if (modalOverlay) {
        modalOverlay.addEventListener('click', function(e) {
            if (e.target === modalOverlay) closeModal();
        });
    }
    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') closeModal();
    });

    // --- Attach click handlers to server-rendered installed cards ---
    function setupCardClicks() {
        document.querySelectorAll('#installed-grid .skill-card[data-skill-name]').forEach(function(card) {
            card.addEventListener('click', function() {
                var name = card.getAttribute('data-skill-name');
                if (name) openInstalledDetail(name);
            });
        });
    }

    // --- Init ---
    setupRemoveButtons();
    setupCardClicks();

})();
