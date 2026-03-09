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
    var sandboxBadgeEl = document.getElementById('skills-sandbox-runtime-badge');
    var sandboxTextEl = document.getElementById('skills-sandbox-runtime-text');
    var refreshSandboxBtn = document.getElementById('skills-refresh-sandbox-status-btn');
    var skillsState = {
        searchResults: [],
        searchQuery: '',
        showAlternatives: false,
    };

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

    function renderSandboxStatus(status) {
        if (!sandboxBadgeEl || !sandboxTextEl) return;
        if (!status) {
            sandboxBadgeEl.textContent = 'unknown';
            sandboxBadgeEl.classList.remove('badge-success', 'badge-warning', 'badge-error');
            sandboxBadgeEl.classList.add('badge-neutral');
            sandboxTextEl.textContent = 'Unable to determine execution sandbox status.';
            return;
        }

        sandboxBadgeEl.textContent = status.enabled
            ? ('resolved: ' + status.resolved_backend)
            : 'disabled';
        sandboxBadgeEl.classList.remove('badge-success', 'badge-warning', 'badge-error', 'badge-neutral');
        if (!status.enabled) {
            sandboxBadgeEl.classList.add('badge-neutral');
        } else if (!status.valid) {
            sandboxBadgeEl.classList.add('badge-error');
        } else if (status.fallback_to_native) {
            sandboxBadgeEl.classList.add('badge-warning');
        } else {
            sandboxBadgeEl.classList.add('badge-success');
        }

        var dockerText = status.docker_available ? 'available' : 'unavailable';
        sandboxTextEl.textContent = (status.message || 'Sandbox status updated.') + ' Docker: ' + dockerText + '.';
    }

    function loadSandboxStatus() {
        return fetch('/api/v1/security/sandbox/status')
            .then(function(r) { return r.ok ? r.json() : null; })
            .then(function(status) {
                renderSandboxStatus(status);
            })
            .catch(function() {
                renderSandboxStatus(null);
            });
    }
    loadSandboxStatus();
    if (refreshSandboxBtn) {
        refreshSandboxBtn.addEventListener('click', function() {
            loadSandboxStatus().then(function() {
                showToast('Sandbox status refreshed', 'success');
            });
        });
    }

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

    function escapeHtml(value) {
        if (value === null || value === undefined) return '';
        return String(value)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
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
        if (searchGrid) {
            searchGrid.textContent = '';
            searchGrid.className = 'skill-list';
        }
        skillsState.searchResults = [];
        skillsState.searchQuery = '';
        skillsState.showAlternatives = false;
    }

    async function doSearch(query) {
        if (searchSpinner) searchSpinner.style.display = 'block';
        try {
            var res = await fetch('/api/v1/skills/search?q=' + encodeURIComponent(query));
            var results = await res.json();
            skillsState.searchResults = Array.isArray(results) ? results : [];
            skillsState.searchQuery = query;
            skillsState.showAlternatives = false;
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
        searchGrid.className = 'skills-search-results-panel';

        if (results.length === 0) {
            if (searchCount) searchCount.textContent = '0 found';
            var empty = el('div', 'empty-state');
            empty.appendChild(el('p', null, 'No skills found. Try a different search term.'));
            searchGrid.appendChild(empty);
            return;
        }

        var recommendedIndex = results.findIndex(function(skill) { return !!skill.recommended; });
        if (recommendedIndex < 0) recommendedIndex = 0;
        var recommended = results[recommendedIndex];
        var alternatives = results.filter(function(_, idx) { return idx !== recommendedIndex; });
        if (searchCount) {
            searchCount.textContent = '1 recommended' + (alternatives.length ? (' · ' + alternatives.length + ' alternatives') : '');
        }

        var recommendationMeta = [];
        recommendationMeta.push('<span class="badge badge-info">Recommended choice</span>');
        if (recommended.source) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(sourceLabel(recommended.source)) + '</span>');
        }
        if (recommended.downloads > 0) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(formatNum(recommended.downloads)) + ' downloads</span>');
        }
        if (recommended.stars > 0) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(formatNum(recommended.stars)) + ' stars</span>');
        }

        var html = '' +
            '<section class="mcp-decision-shell">' +
                '<div class="mcp-decision-header">' +
                    '<div class="mcp-recommendation-label">Best match for "' + escapeHtml(skillsState.searchQuery) + '"</div>' +
                    '<div class="mcp-recommendation-meta">' + recommendationMeta.join(' ') + '</div>' +
                '</div>' +
                '<div class="mcp-decision-lead">' + escapeHtml(recommended.recommended_reason || 'Best overall match for this search.') + '</div>' +
                '<div class="mcp-decision-card-wrap">' +
                    buildSearchCardHtml(recommended, 'featured') +
                '</div>' +
                (alternatives.length
                    ? '<div class="mcp-recommendation-actions">' +
                        '<button type="button" class="btn btn-secondary btn-sm skills-toggle-alternatives-btn" aria-expanded="' + (skillsState.showAlternatives ? 'true' : 'false') + '">' +
                            (skillsState.showAlternatives ? 'Hide alternatives' : ('Show alternatives (' + alternatives.length + ')')) +
                        '</button>' +
                      '</div>'
                    : '') +
            '</section>';

        if (alternatives.length && skillsState.showAlternatives) {
            html += '' +
                '<section class="mcp-alternatives">' +
                    '<div class="mcp-alternatives-header">Alternative skills</div>' +
                    '<div class="skill-list mcp-skill-list mcp-alternatives-grid">' +
                        alternatives.map(function(skill) {
                            return buildSearchCardHtml(skill, 'alternative');
                        }).join('') +
                    '</div>' +
                '</section>';
        }

        searchGrid.innerHTML = html;
        bindSearchResultActions();
    }

    function buildSearchCardHtml(skill, mode) {
        mode = mode || 'default';
        var displayName = skill.name;
        if (displayName.indexOf('clawhub:') === 0) displayName = displayName.substring(8);
        if (displayName.indexOf('openskills:') === 0) displayName = displayName.substring(11);

        var skillBaseName = displayName.split('/').pop() || displayName;
        var isInstalled = installedNames.has(skillBaseName);
        var decisionTags = (skill.decision_tags || []).length
            ? '<div class="mcp-card-tags">' + skill.decision_tags.map(function(tag) {
                return '<span class="badge badge-neutral">' + escapeHtml(tag) + '</span>';
            }).join('') + '</div>'
            : '';
        var rationale = '';
        if (mode === 'alternative' && (skill.why_choose || skill.tradeoff)) {
            rationale = '' +
                '<div class="mcp-card-rationale mcp-card-rationale--compact">' +
                    (skill.why_choose ? '<div><strong>Why choose this</strong> ' + escapeHtml(skill.why_choose) + '</div>' : '') +
                    (skill.tradeoff ? '<div><strong>Tradeoff</strong> ' + escapeHtml(skill.tradeoff) + '</div>' : '') +
                '</div>';
        }
        var stats = '';
        if (skill.stars > 0 || skill.downloads > 0) {
            var parts = [];
            if (skill.stars > 0) {
                parts.push('<span class="skill-stat">' + svgStar().outerHTML + ' ' + escapeHtml(formatNum(skill.stars)) + '</span>');
            }
            if (skill.downloads > 0) {
                parts.push('<span class="skill-stat">' + svgDownload().outerHTML + ' ' + escapeHtml(formatNum(skill.downloads)) + '</span>');
            }
            stats = '<div class="skill-meta">' + parts.join('') + '</div>';
        }
        return '' +
            '<div class="skill-card mcp-catalog-card mcp-catalog-card--decision ' +
                (mode === 'featured' ? 'mcp-catalog-card--featured ' : '') +
                (mode === 'alternative' ? 'mcp-catalog-card--alternative ' : '') +
                '" data-skill-source="' + escapeHtml(skill.name) + '" data-skill-display-name="' + escapeHtml(displayName) + '" data-skill-base-name="' + escapeHtml(skillBaseName) + '">' +
                '<div class="skill-card-header">' +
                    '<div class="skill-name">' + escapeHtml(displayName) + (skill.recommended ? '<span class="mcp-card-flag">Recommended</span>' : '') + '</div>' +
                    '<span class="skill-source-badge skill-source-badge--' + escapeHtml(skill.source) + '">' + escapeHtml(sourceLabel(skill.source)) + '</span>' +
                '</div>' +
                '<div class="skill-desc">' + escapeHtml(skill.description || 'No description') + '</div>' +
                decisionTags +
                stats +
                rationale +
                '<div class="skill-card-footer">' +
                    '<span class="skill-path">' + escapeHtml(skill.name) + '</span>' +
                    '<div class="skill-card-actions">' +
                        (isInstalled
                            ? '<button type="button" class="btn btn-sm btn-installed" disabled>Installed</button>'
                            : '<button type="button" class="btn btn-sm btn-primary skills-install-btn" data-skill-source="' + escapeHtml(skill.name) + '">Install</button>') +
                    '</div>' +
                '</div>' +
            '</div>';
    }

    function bindSearchResultActions() {
        if (!searchGrid) return;
        searchGrid.querySelectorAll('.skills-install-btn').forEach(function(btn) {
            btn.addEventListener('click', function(e) {
                e.stopPropagation();
                var source = btn.getAttribute('data-skill-source');
                var card = btn.closest('.skill-card');
                if (!source) return;
                installSkill(source, btn, card);
            });
        });
        searchGrid.querySelectorAll('.skill-card[data-skill-source]').forEach(function(card) {
            card.addEventListener('click', function() {
                var source = card.getAttribute('data-skill-source');
                var displayName = card.getAttribute('data-skill-display-name') || source;
                var skillBaseName = card.getAttribute('data-skill-base-name') || displayName;
                var skill = skillsState.searchResults.find(function(item) { return item.name === source; });
                if (!skill) return;
                if (installedNames.has(skillBaseName)) {
                    openInstalledDetail(skillBaseName);
                } else {
                    openSearchDetail(skill, displayName);
                }
            });
        });
        searchGrid.querySelectorAll('.skills-toggle-alternatives-btn').forEach(function(btn) {
            btn.addEventListener('click', function() {
                skillsState.showAlternatives = !skillsState.showAlternatives;
                renderSearchResults(skillsState.searchResults);
            });
        });
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
                var installMessage = 'Installed: ' + data.name;
                if (data.security_report && data.security_report.warnings > 0) {
                    installMessage += ' (risk ' + data.security_report.risk_score + '/100)';
                }
                showToast(installMessage, 'success');
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
                var installMessage = 'Installed: ' + data.name;
                if (data.security_report && data.security_report.warnings > 0) {
                    installMessage += ' (risk ' + data.security_report.risk_score + '/100)';
                }
                showToast(installMessage, 'success');
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

            addScanButton(detail.name, modalFooter);

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

    // --- Skill Creator ---

    var creatorPanel = document.getElementById('skill-creator-panel');
    var creatorToggleBtn = document.getElementById('create-skill-toggle-btn');
    var creatorSubmitBtn = document.getElementById('creator-submit-btn');
    var creatorCancelBtn = document.getElementById('creator-cancel-btn');
    var creatorSpinner = document.getElementById('creator-spinner');
    var creatorResult = document.getElementById('creator-result');
    var creatorPrompt = document.getElementById('creator-prompt');

    function toggleCreatorPanel() {
        if (!creatorPanel) return;
        var visible = creatorPanel.style.display !== 'none';
        creatorPanel.style.display = visible ? 'none' : 'block';
        if (!visible && creatorPrompt) creatorPrompt.focus();
    }

    if (creatorToggleBtn) creatorToggleBtn.addEventListener('click', toggleCreatorPanel);
    if (creatorCancelBtn) creatorCancelBtn.addEventListener('click', function() {
        if (creatorPanel) creatorPanel.style.display = 'none';
    });

    async function submitCreateSkill() {
        var prompt = (document.getElementById('creator-prompt').value || '').trim();
        if (!prompt) {
            showToast('Please describe what the skill should do', 'error');
            return;
        }

        var name = (document.getElementById('creator-name').value || '').trim() || undefined;
        var language = document.getElementById('creator-language').value || undefined;
        var overwrite = document.getElementById('creator-overwrite').checked;

        if (creatorSubmitBtn) { creatorSubmitBtn.disabled = true; creatorSubmitBtn.textContent = 'Creating...'; }
        if (creatorSpinner) creatorSpinner.style.display = 'inline-block';
        if (creatorResult) creatorResult.style.display = 'none';

        try {
            var res = await fetch('/api/v1/skills/create', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ prompt: prompt, name: name, language: language, overwrite: overwrite }),
            });
            var data = await res.json();

            if (data.ok) {
                showToast('Skill created: ' + data.name, 'success');
                installedNames.add(data.name);
                renderCreatorResult(data);
                refreshInstalled();
            } else {
                showToast('Creation failed: ' + data.message, 'error');
                if (creatorResult) {
                    creatorResult.textContent = '';
                    var errCard = el('div', 'creator-result-card');
                    var errP = el('p');
                    errP.style.color = 'var(--err)';
                    errP.textContent = data.message;
                    errCard.appendChild(errP);
                    creatorResult.appendChild(errCard);
                    creatorResult.style.display = 'block';
                }
            }
        } catch (err) {
            showToast('Creation failed: ' + err.message, 'error');
        } finally {
            if (creatorSubmitBtn) { creatorSubmitBtn.disabled = false; creatorSubmitBtn.textContent = 'Create Skill'; }
            if (creatorSpinner) creatorSpinner.style.display = 'none';
        }
    }

    if (creatorSubmitBtn) creatorSubmitBtn.addEventListener('click', submitCreateSkill);

    function renderCreatorResult(data) {
        if (!creatorResult) return;
        creatorResult.textContent = '';

        var card = el('div', 'creator-result-card');

        // Header: name + badges
        var header = el('div', 'creator-result-header');
        header.appendChild(el('div', 'skill-name', data.name));
        header.appendChild(el('span', 'badge badge-neutral', data.language));
        header.appendChild(el('span',
            data.smoke_test_passed ? 'badge badge-success' : 'badge badge-warning',
            data.smoke_test_passed ? 'smoke test passed' : 'smoke test skipped'));
        card.appendChild(header);

        // Path
        var metaDiv = el('div', 'creator-result-meta');
        var pathCode = el('code');
        pathCode.style.fontSize = '11px';
        pathCode.style.color = 'var(--t3)';
        pathCode.textContent = data.path;
        metaDiv.appendChild(pathCode);
        card.appendChild(metaDiv);

        // Reused skills
        if (data.reused_skills && data.reused_skills.length > 0) {
            var reusedMeta = el('div', 'creator-result-meta');
            reusedMeta.appendChild(el('span', 'badge badge-neutral', 'reused: ' + data.reused_skills.join(', ')));
            card.appendChild(reusedMeta);
        }

        // Validation notes
        if (data.validation_notes && data.validation_notes.length > 0) {
            var notesDiv = el('div', 'creator-result-notes');
            notesDiv.appendChild(el('strong', null, 'Notes:'));
            var notesList = document.createElement('ul');
            data.validation_notes.forEach(function(n) {
                notesList.appendChild(el('li', null, n));
            });
            notesDiv.appendChild(notesList);
            card.appendChild(notesDiv);
        }

        // Security report
        if (data.security_report) {
            card.appendChild(buildSecurityReportEl(data.security_report));
        }

        creatorResult.appendChild(card);
        creatorResult.style.display = 'block';
    }

    // --- Security Report Renderer (shared: creator result + modal scan) ---

    function riskBadgeClass(score) {
        if (score < 20) return 'risk-badge--low';
        if (score < 65) return 'risk-badge--medium';
        return 'risk-badge--high';
    }

    function riskLabel(score, blocked) {
        if (blocked) return 'BLOCKED (' + score + '/100)';
        if (score === 0) return 'Clean (0/100)';
        return 'Risk ' + score + '/100';
    }

    function buildSecurityReportEl(report) {
        var container = el('div', 'security-report');

        // Header
        var headerDiv = el('div', 'security-report-header');
        var shieldSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        shieldSvg.setAttribute('viewBox', '0 0 16 16');
        shieldSvg.setAttribute('fill', 'none');
        shieldSvg.setAttribute('stroke', 'currentColor');
        shieldSvg.setAttribute('stroke-width', '1.5');
        shieldSvg.style.width = '14px';
        shieldSvg.style.height = '14px';
        var shieldPath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        shieldPath.setAttribute('d', 'M8 1L14.5 4.5V8c0 3.5-2.5 6-6.5 7-4-1-6.5-3.5-6.5-7V4.5z');
        shieldSvg.appendChild(shieldPath);
        headerDiv.appendChild(shieldSvg);
        headerDiv.appendChild(el('strong', null, 'Security Scan'));
        headerDiv.appendChild(el('span', 'badge ' + riskBadgeClass(report.risk_score), riskLabel(report.risk_score, report.blocked)));
        container.appendChild(headerDiv);

        // Summary
        container.appendChild(el('div', 'security-report-summary', report.summary));

        // Warnings list
        if (report.warnings && report.warnings.length > 0) {
            var list = document.createElement('ul');
            list.className = 'security-warnings-list';
            report.warnings.forEach(function(w) {
                var row = el('li', 'security-warning-row');
                row.appendChild(el('span', 'security-warning-severity security-warning-severity--' + w.severity, w.severity));
                row.appendChild(el('span', 'security-warning-desc', w.description));
                if (w.file) {
                    var loc = w.file;
                    if (w.line) loc += ':' + w.line;
                    row.appendChild(el('span', 'security-warning-location', loc));
                }
                list.appendChild(row);
            });
            container.appendChild(list);
        }

        return container;
    }

    // --- Security Scan in Modal ---

    function addScanButton(name, footerEl) {
        var scanBtn = el('button', 'btn btn-sm btn-secondary', 'Security Scan');
        scanBtn.addEventListener('click', async function(e) {
            e.stopPropagation();
            scanBtn.textContent = 'Scanning...';
            scanBtn.disabled = true;
            try {
                var res = await fetch('/api/v1/skills/' + encodeURIComponent(name) + '/scan', { method: 'POST' });
                var data = await res.json();
                if (data.ok && data.report) {
                    var existing = modalContent.querySelector('.security-report');
                    if (existing) existing.remove();
                    modalContent.appendChild(buildSecurityReportEl(data.report));
                    showToast('Scan complete: risk ' + data.report.risk_score + '/100', 'success');
                } else {
                    showToast('Scan failed: ' + (data.message || 'Unknown error'), 'error');
                }
            } catch (err) {
                showToast('Scan failed: ' + err.message, 'error');
            } finally {
                scanBtn.textContent = 'Security Scan';
                scanBtn.disabled = false;
            }
        });
        footerEl.appendChild(scanBtn);
    }

    // --- Init ---
    setupRemoveButtons();
    setupCardClicks();

})();
