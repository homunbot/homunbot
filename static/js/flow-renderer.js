/**
 * Homun — Visual Flow Renderer (n8n-style)
 *
 * Two rendering modes:
 *   renderFlowMini(container, flowData)  — tiny dot strip with hover tooltips
 *   renderFlow(container, flowData)       — full n8n-style dark canvas
 *
 * No external dependencies.
 */

(function () {
    'use strict';

    var NS = 'http://www.w3.org/2000/svg';

    // ─── Node kind → visual config ────────────────────────────────
    // Colors are for the n8n-style dark canvas (node card bg is dark, accent for icon bg)

    var KIND_CONFIG = {
        trigger: {
            accent: '#E8A838',   // warm amber
            icon: 'M13 10V3L4 14h7v7l9-11h-7z', // lightning bolt
            iconFill: true,
        },
        tool: {
            accent: '#68B984',
            icon: 'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6-7.6 7.6-1.6-1.6a1 1 0 1 0-1.4 1.4l2.3 2.3a1 1 0 0 0 1.4 0l8.3-8.3a1 1 0 0 0 0-1.4l-2.3-2.3a1 1 0 0 0-1.4 0z',
            iconFill: false,
        },
        skill: {
            accent: '#E07C4F',   // terracotta
            icon: 'M13 10V3L4 14h7v7l9-11h-7z',
            iconFill: true,
        },
        mcp: {
            accent: '#9B72CF',   // plum
            icon: 'M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10 10-4.5 10-10S17.5 2 12 2zm0 4a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm-4 8a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm8 0a2 2 0 1 1 0 4 2 2 0 0 1 0-4z',
            iconFill: true,
        },
        llm: {
            accent: '#5B9BD5',   // soft blue
            icon: 'M12 2a9 9 0 0 0-9 9c0 3.1 1.6 5.9 4 7.5V21a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-2.5c2.4-1.6 4-4.4 4-7.5a9 9 0 0 0-9-9z',
            iconFill: true,
        },
        condition: {
            accent: '#8BC34A',   // green
            icon: '',
            shape: 'diamond',
        },
        parallel: {
            accent: '#26A69A',   // teal
            icon: '',
            shape: 'diamond',
        },
        subprocess: {
            accent: '#5C7AEA',   // indigo
            icon: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
            iconFill: true,
            shape: 'subprocess',
        },
        loop: {
            accent: '#AB8F67',
            icon: 'M17.65 6.35A8 8 0 1 0 20 12h-2a6 6 0 1 1-1.76-4.24L13 11h7V4z',
            iconFill: true,
        },
        transform: {
            accent: '#78909C',   // blue-grey
            icon: 'M12 15.5A3.5 3.5 0 1 1 12 8.5a3.5 3.5 0 0 1 0 7z',
            iconFill: true,
        },
        deliver: {
            accent: '#42A5F5',   // info blue
            icon: 'M2 21l21-9L2 3v7l15 2-15 2v7z',
            iconFill: true,
        },
    };

    var DEFAULT_CONFIG = { accent: '#888', icon: '', iconFill: false };

    // ─── Canvas constants ─────────────────────────────────────────

    // Full canvas node
    var NODE_W = 160;
    var NODE_H = 72;
    var DIAMOND_S = 56;
    var NODE_RX = 12;
    var GAP_X = 70;
    var GAP_Y = 36;
    var PAD = 40;
    var ICON_SIZE = 32;
    var CONNECTOR_R = 5;

    // Canvas colors
    var CANVAS_BG = '#1E1F2B';
    var CANVAS_BG_LIGHT = '#F0EDE6';
    var NODE_BG = '#2A2B3D';
    var NODE_BG_LIGHT = '#FFFFFF';
    var NODE_BORDER = '#383A4E';
    var NODE_BORDER_LIGHT = '#DDD8CB';
    var NODE_TEXT = '#E8E6E3';
    var NODE_TEXT_LIGHT = '#2C2924';
    var NODE_META_TEXT = '#8B8A95';
    var NODE_META_TEXT_LIGHT = '#8A847A';
    var EDGE_COLOR = '#4A4B5C';
    var EDGE_COLOR_LIGHT = '#CEC7B8';
    var GRID_DOT = '#2A2C3A';
    var GRID_DOT_LIGHT = '#E4E0D6';

    // Mini strip
    var MINI_R = 7;
    var MINI_GAP = 22;
    var MINI_PAD = 6;

    // ─── SVG helpers ──────────────────────────────────────────────

    function svgEl(tag, attrs) {
        var el = document.createElementNS(NS, tag);
        if (attrs) {
            for (var k in attrs) {
                if (Object.prototype.hasOwnProperty.call(attrs, k)) {
                    el.setAttribute(k, attrs[k]);
                }
            }
        }
        return el;
    }

    function clearContainer(el) {
        while (el.firstChild) el.removeChild(el.firstChild);
    }

    function truncStr(s, max) {
        if (!s || s.length <= max) return s || '';
        return s.substring(0, max - 1) + '\u2026';
    }

    function isDark() {
        return document.documentElement.classList.contains('dark');
    }

    function canvasBg()   { return isDark() ? CANVAS_BG   : CANVAS_BG_LIGHT; }
    function nodeBg()     { return isDark() ? NODE_BG     : NODE_BG_LIGHT; }
    function nodeBorder() { return isDark() ? NODE_BORDER : NODE_BORDER_LIGHT; }
    function nodeText()   { return isDark() ? NODE_TEXT   : NODE_TEXT_LIGHT; }
    function nodeMeta()   { return isDark() ? NODE_META_TEXT : NODE_META_TEXT_LIGHT; }
    function edgeColor()  { return isDark() ? EDGE_COLOR  : EDGE_COLOR_LIGHT; }
    function gridDot()    { return isDark() ? GRID_DOT    : GRID_DOT_LIGHT; }

    // ─── DAG Layout Engine ────────────────────────────────────────

    function buildGraph(nodes, edges) {
        var nodeMap = {}, children = {}, parents = {};
        nodes.forEach(function (n) {
            nodeMap[n.id] = n; children[n.id] = []; parents[n.id] = [];
        });
        edges.forEach(function (e) {
            if (nodeMap[e.from] && nodeMap[e.to]) {
                children[e.from].push(e.to);
                parents[e.to].push(e.from);
            }
        });
        return { nodeMap: nodeMap, children: children, parents: parents };
    }

    function layoutDAG(nodes, edges) {
        var g = buildGraph(nodes, edges);
        var rank = {}, lane = {}, positions = {};

        // Topological rank via BFS
        var inDeg = {};
        nodes.forEach(function (n) { inDeg[n.id] = g.parents[n.id].length; });
        var queue = [];
        nodes.forEach(function (n) {
            if (inDeg[n.id] === 0) { queue.push(n.id); rank[n.id] = 0; }
        });
        var order = [];
        while (queue.length > 0) {
            var cur = queue.shift();
            order.push(cur);
            g.children[cur].forEach(function (child) {
                rank[child] = Math.max(rank[child] || 0, rank[cur] + 1);
                inDeg[child]--;
                if (inDeg[child] === 0) queue.push(child);
            });
        }
        nodes.forEach(function (n) {
            if (rank[n.id] === undefined) { rank[n.id] = 0; order.push(n.id); }
        });

        // Assign lanes for parallel branches
        nodes.forEach(function (n) { lane[n.id] = 0; });
        order.forEach(function (id) {
            var ch = g.children[id];
            if (ch.length <= 1) return;
            var parentLane = lane[id];
            ch.forEach(function (c, i) {
                var offset = i - (ch.length - 1) / 2;
                lane[c] = parentLane + offset;
                propagateLane(c, parentLane + offset, g, lane);
            });
        });

        // Normalize lanes to 0-based
        var minLane = 0;
        nodes.forEach(function (n) { if (lane[n.id] < minLane) minLane = lane[n.id]; });
        if (minLane < 0) nodes.forEach(function (n) { lane[n.id] -= minLane; });
        var maxLane = 0;
        nodes.forEach(function (n) { if (lane[n.id] > maxLane) maxLane = lane[n.id]; });

        var maxRank = 0;
        nodes.forEach(function (n) { if (rank[n.id] > maxRank) maxRank = rank[n.id]; });

        // Pixel positions
        nodes.forEach(function (n) {
            var cfg = KIND_CONFIG[n.kind] || DEFAULT_CONFIG;
            var isDiamond = cfg.shape === 'diamond';
            var w = isDiamond ? DIAMOND_S : NODE_W;
            var h = isDiamond ? DIAMOND_S : NODE_H;
            var xOff = isDiamond ? (NODE_W - DIAMOND_S) / 2 : 0;
            var yOff = isDiamond ? (NODE_H - DIAMOND_S) / 2 : 0;

            positions[n.id] = {
                x: PAD + rank[n.id] * (NODE_W + GAP_X) + xOff,
                y: PAD + lane[n.id] * (NODE_H + GAP_Y) + yOff,
                w: w, h: h,
                rank: rank[n.id],
                lane: lane[n.id],
                shape: isDiamond ? 'diamond' : (cfg.shape || 'rect'),
            };
        });

        return {
            positions: positions,
            totalW: PAD * 2 + (maxRank + 1) * NODE_W + maxRank * GAP_X,
            totalH: PAD * 2 + (maxLane + 1) * NODE_H + maxLane * GAP_Y,
        };
    }

    function propagateLane(nodeId, laneVal, g, lane) {
        var ch = g.children[nodeId];
        if (!ch) return;
        ch.forEach(function (c) {
            if (g.parents[c].length === 1) {
                lane[c] = laneVal;
                propagateLane(c, laneVal, g, lane);
            }
        });
    }

    // ─── Edge path helpers ────────────────────────────────────────

    function getOutputXY(pos) {
        if (pos.shape === 'diamond') return { x: pos.x + pos.w, y: pos.y + pos.h / 2 };
        return { x: pos.x + pos.w, y: pos.y + pos.h / 2 };
    }

    function getInputXY(pos) {
        if (pos.shape === 'diamond') return { x: pos.x, y: pos.y + pos.h / 2 };
        return { x: pos.x, y: pos.y + pos.h / 2 };
    }

    function bezierPath(x1, y1, x2, y2) {
        var dx = Math.max(Math.abs(x2 - x1) * 0.45, 40);
        return 'M' + x1 + ',' + y1 +
            ' C' + (x1 + dx) + ',' + y1 +
            ' ' + (x2 - dx) + ',' + y2 +
            ' ' + x2 + ',' + y2;
    }

    // ─── SVG Definitions ─────────────────────────────────────────

    function addDefs(svg) {
        var defs = svgEl('defs');

        // Drop shadow for nodes
        var filter = svgEl('filter', { id: 'n8n-shadow', x: '-8%', y: '-8%', width: '116%', height: '132%' });
        var flood = svgEl('feFlood', { 'flood-color': 'rgba(0,0,0,0.25)', result: 'shadow' });
        var offset = svgEl('feOffset', { in: 'shadow', dx: '0', dy: '3', result: 'off' });
        var blur = svgEl('feGaussianBlur', { in: 'off', stdDeviation: '5', result: 'blur' });
        var merge = svgEl('feMerge');
        merge.appendChild(svgEl('feMergeNode', { in: 'blur' }));
        merge.appendChild(svgEl('feMergeNode', { in: 'SourceGraphic' }));
        filter.appendChild(flood); filter.appendChild(offset);
        filter.appendChild(blur); filter.appendChild(merge);
        defs.appendChild(filter);

        // Arrow marker for edges
        var marker = svgEl('marker', {
            id: 'n8n-arrow', viewBox: '0 0 10 8', refX: '10', refY: '4',
            markerWidth: '8', markerHeight: '6', orient: 'auto-start-reverse',
        });
        marker.appendChild(svgEl('path', { d: 'M0 0 L10 4 L0 8 z', fill: edgeColor() }));
        defs.appendChild(marker);

        // Dot grid pattern
        var pattern = svgEl('pattern', {
            id: 'n8n-grid', x: '0', y: '0', width: '20', height: '20',
            patternUnits: 'userSpaceOnUse',
        });
        pattern.appendChild(svgEl('circle', {
            cx: '10', cy: '10', r: '1', fill: gridDot(),
        }));
        defs.appendChild(pattern);

        svg.appendChild(defs);
    }

    // ─── Node renderers (n8n-style) ──────────────────────────────

    function renderRectNode(gEl, node, pos, cfg) {
        var bg = nodeBg();
        var border = nodeBorder();
        var accent = cfg.accent;

        // Card background
        gEl.appendChild(svgEl('rect', {
            x: '0', y: '0', width: pos.w, height: pos.h,
            rx: NODE_RX, fill: bg,
            stroke: border, 'stroke-width': '1.5',
            filter: 'url(#n8n-shadow)',
        }));

        // Left accent bar
        // Clip to rounded rect by using a small rect at left inside the card
        var bar = svgEl('rect', {
            x: '0', y: '0', width: '4', height: pos.h,
            rx: NODE_RX + ' 0 0 ' + NODE_RX,
            fill: accent,
        });
        // Use clipPath approach: just overlay a thin accent bar
        gEl.appendChild(svgEl('rect', {
            x: '0', y: '1', width: '4', height: pos.h - 2,
            fill: accent,
        }));

        // Icon background square (rounded)
        var iconX = 14;
        var iconY = (pos.h - ICON_SIZE) / 2;
        gEl.appendChild(svgEl('rect', {
            x: iconX, y: iconY, width: ICON_SIZE, height: ICON_SIZE,
            rx: '8', fill: accent,
        }));

        // Icon SVG
        if (cfg.icon) {
            var scale = (ICON_SIZE - 12) / 24;
            var iconG = svgEl('g', {
                transform: 'translate(' + (iconX + 6) + ',' + (iconY + 6) + ') scale(' + scale + ')',
            });
            var pathAttrs = { d: cfg.icon };
            if (cfg.iconFill) {
                pathAttrs.fill = '#FFFFFF';
                pathAttrs.stroke = 'none';
            } else {
                pathAttrs.fill = 'none';
                pathAttrs.stroke = '#FFFFFF';
                pathAttrs['stroke-width'] = '2';
                pathAttrs['stroke-linecap'] = 'round';
                pathAttrs['stroke-linejoin'] = 'round';
            }
            iconG.appendChild(svgEl('path', pathAttrs));
            gEl.appendChild(iconG);
        }

        // Label text
        var textX = iconX + ICON_SIZE + 10;
        var labelY = node.meta ? pos.h / 2 - 7 : pos.h / 2;
        var label = svgEl('text', {
            x: textX, y: labelY,
            'dominant-baseline': 'middle',
            fill: nodeText(),
            'font-size': '12px',
            'font-weight': '600',
            'font-family': 'inherit',
        });
        label.textContent = truncStr(node.label, 12);
        gEl.appendChild(label);

        // Meta/subtitle text
        if (node.meta) {
            var meta = svgEl('text', {
                x: textX, y: pos.h / 2 + 9,
                'dominant-baseline': 'middle',
                fill: nodeMeta(),
                'font-size': '10px',
                'font-family': 'inherit',
            });
            meta.textContent = truncStr(node.meta, 16);
            gEl.appendChild(meta);
        }

        // Input connector dot (left center)
        gEl.appendChild(svgEl('circle', {
            cx: '0', cy: pos.h / 2, r: CONNECTOR_R,
            fill: bg, stroke: border, 'stroke-width': '1.5',
            class: 'flow-connector',
        }));
        // Output connector dot (right center)
        gEl.appendChild(svgEl('circle', {
            cx: pos.w, cy: pos.h / 2, r: CONNECTOR_R,
            fill: bg, stroke: border, 'stroke-width': '1.5',
            class: 'flow-connector',
        }));
    }

    function renderDiamondNode(gEl, node, pos, cfg) {
        var bg = nodeBg();
        var accent = cfg.accent;
        var s = pos.w;
        var h = s / 2;

        // Diamond shape
        gEl.appendChild(svgEl('polygon', {
            points: h + ',0 ' + s + ',' + h + ' ' + h + ',' + s + ' 0,' + h,
            fill: accent,
            stroke: 'rgba(255,255,255,0.15)',
            'stroke-width': '1.5',
            filter: 'url(#n8n-shadow)',
        }));

        // "IF" or label text
        var text = svgEl('text', {
            x: h, y: h + 1,
            'text-anchor': 'middle',
            'dominant-baseline': 'middle',
            fill: '#FFFFFF',
            'font-size': '12px',
            'font-weight': '700',
            'font-family': 'inherit',
        });
        text.textContent = truncStr(node.label, 6);
        gEl.appendChild(text);

        // Connector dots
        gEl.appendChild(svgEl('circle', {
            cx: '0', cy: h, r: CONNECTOR_R,
            fill: bg, stroke: accent, 'stroke-width': '1.5',
        }));
        gEl.appendChild(svgEl('circle', {
            cx: s, cy: h, r: CONNECTOR_R,
            fill: bg, stroke: accent, 'stroke-width': '1.5',
        }));
    }

    function renderSubprocessNode(gEl, node, pos, cfg) {
        var bg = nodeBg();
        var border = nodeBorder();
        var accent = cfg.accent;

        // Double-border card
        gEl.appendChild(svgEl('rect', {
            x: '0', y: '0', width: pos.w, height: pos.h,
            rx: NODE_RX, fill: bg,
            stroke: accent, 'stroke-width': '2',
            filter: 'url(#n8n-shadow)',
        }));
        gEl.appendChild(svgEl('rect', {
            x: '4', y: '4', width: pos.w - 8, height: pos.h - 8,
            rx: NODE_RX - 2, fill: 'none',
            stroke: accent, 'stroke-width': '1',
            opacity: '0.4', 'stroke-dasharray': '4 3',
        }));

        // Label
        var label = svgEl('text', {
            x: pos.w / 2, y: pos.h / 2,
            'text-anchor': 'middle',
            'dominant-baseline': 'middle',
            fill: nodeText(),
            'font-size': '12px',
            'font-weight': '600',
            'font-family': 'inherit',
        });
        label.textContent = truncStr(node.label, 14);
        gEl.appendChild(label);

        // Connectors
        gEl.appendChild(svgEl('circle', {
            cx: '0', cy: pos.h / 2, r: CONNECTOR_R,
            fill: bg, stroke: accent, 'stroke-width': '1.5',
        }));
        gEl.appendChild(svgEl('circle', {
            cx: pos.w, cy: pos.h / 2, r: CONNECTOR_R,
            fill: bg, stroke: accent, 'stroke-width': '1.5',
        }));
    }

    // ─── Full canvas renderer ─────────────────────────────────────

    function renderFlow(container, flowData) {
        if (!flowData || !flowData.nodes || !flowData.nodes.length) return;
        clearContainer(container);

        var nodes = flowData.nodes;
        var edges = flowData.edges || [];
        var layout = layoutDAG(nodes, edges);
        var positions = layout.positions;

        // Create SVG
        var svg = svgEl('svg', {
            width: layout.totalW,
            height: layout.totalH,
            viewBox: '0 0 ' + layout.totalW + ' ' + layout.totalH,
            class: 'flow-svg',
        });
        svg.style.display = 'block';
        addDefs(svg);

        // Canvas background with dot grid
        svg.appendChild(svgEl('rect', {
            width: layout.totalW, height: layout.totalH,
            fill: canvasBg(),
            rx: '8',
        }));
        svg.appendChild(svgEl('rect', {
            width: layout.totalW, height: layout.totalH,
            fill: 'url(#n8n-grid)',
            rx: '8',
        }));

        // Render edges
        var edgesG = svgEl('g', { class: 'flow-edges' });
        edges.forEach(function (e) {
            var from = positions[e.from];
            var to = positions[e.to];
            if (!from || !to) return;

            var out = getOutputXY(from);
            var inp = getInputXY(to);
            var eG = svgEl('g', { class: 'flow-edge' });

            eG.appendChild(svgEl('path', {
                d: bezierPath(out.x, out.y, inp.x, inp.y),
                fill: 'none',
                stroke: edgeColor(),
                'stroke-width': '2',
                'marker-end': 'url(#n8n-arrow)',
            }));

            // Edge label (e.g. "true", "false" for condition branches)
            if (e.label) {
                var mx = (out.x + inp.x) / 2;
                var my = (out.y + inp.y) / 2 - 12;

                // Label background pill
                var lblBg = svgEl('rect', {
                    x: mx - 20, y: my - 9, width: '40', height: '18',
                    rx: '9', fill: canvasBg(), stroke: edgeColor(),
                    'stroke-width': '1',
                });
                eG.appendChild(lblBg);

                var lbl = svgEl('text', {
                    x: mx, y: my + 1,
                    'text-anchor': 'middle',
                    'dominant-baseline': 'middle',
                    fill: nodeMeta(),
                    'font-size': '9px',
                    'font-weight': '500',
                    'font-family': 'inherit',
                });
                lbl.textContent = e.label;
                eG.appendChild(lbl);
            }

            edgesG.appendChild(eG);
        });
        svg.appendChild(edgesG);

        // Render nodes
        var nodesG = svgEl('g', { class: 'flow-nodes' });
        nodes.forEach(function (node) {
            var pos = positions[node.id];
            if (!pos) return;
            var cfg = KIND_CONFIG[node.kind] || DEFAULT_CONFIG;

            var gEl = svgEl('g', {
                class: 'flow-node flow-node--' + (pos.shape || 'rect'),
                transform: 'translate(' + pos.x + ',' + pos.y + ')',
                'data-kind': node.kind,
                'data-id': node.id,
            });

            if (pos.shape === 'diamond') renderDiamondNode(gEl, node, pos, cfg);
            else if (pos.shape === 'subprocess') renderSubprocessNode(gEl, node, pos, cfg);
            else renderRectNode(gEl, node, pos, cfg);

            // Tooltip title element (browser-native hover tooltip)
            var title = svgEl('title');
            var tipText = node.kind.toUpperCase() + ': ' + node.label;
            if (node.meta) tipText += '\n' + node.meta;
            title.textContent = tipText;
            gEl.insertBefore(title, gEl.firstChild);

            nodesG.appendChild(gEl);
        });
        svg.appendChild(nodesG);

        container.appendChild(svg);
    }

    // ─── Mini flow strip (for collapsed cards) ────────────────────

    function renderFlowMini(container, flowData) {
        if (!flowData || !flowData.nodes || !flowData.nodes.length) return;
        clearContainer(container);

        var nodes = flowData.nodes;
        var edges = flowData.edges || [];

        // Rank via BFS
        var g = buildGraph(nodes, edges);
        var rank = {};
        var inDeg = {};
        nodes.forEach(function (n) { inDeg[n.id] = g.parents[n.id].length; });
        var queue = [];
        nodes.forEach(function (n) { if (inDeg[n.id] === 0) { queue.push(n.id); rank[n.id] = 0; } });
        while (queue.length > 0) {
            var cur = queue.shift();
            g.children[cur].forEach(function (child) {
                rank[child] = Math.max(rank[child] || 0, rank[cur] + 1);
                inDeg[child]--;
                if (inDeg[child] === 0) queue.push(child);
            });
        }
        nodes.forEach(function (n) { if (rank[n.id] === undefined) rank[n.id] = 0; });

        // Pick one representative node per rank
        var maxRank = 0;
        var rankNodes = {};
        nodes.forEach(function (n) {
            var r = rank[n.id];
            if (r > maxRank) maxRank = r;
            if (!rankNodes[r]) rankNodes[r] = n;
        });
        var uniNodes = [];
        for (var r = 0; r <= maxRank; r++) {
            if (rankNodes[r]) uniNodes.push(rankNodes[r]);
        }

        var totalW = MINI_PAD * 2 + (uniNodes.length - 1) * MINI_GAP + MINI_R * 2;
        var totalH = MINI_PAD * 2 + MINI_R * 2;

        var svg = svgEl('svg', {
            width: totalW, height: totalH,
            viewBox: '0 0 ' + totalW + ' ' + totalH,
            class: 'flow-svg flow-svg--mini',
        });

        // Connecting lines
        for (var i = 0; i < uniNodes.length - 1; i++) {
            svg.appendChild(svgEl('line', {
                x1: MINI_PAD + i * MINI_GAP + MINI_R * 2,
                y1: MINI_PAD + MINI_R,
                x2: MINI_PAD + (i + 1) * MINI_GAP,
                y2: MINI_PAD + MINI_R,
                stroke: isDark() ? 'rgba(255,255,255,0.15)' : 'rgba(0,0,0,0.12)',
                'stroke-width': '1.5',
            }));
        }

        // Circles with tooltips
        uniNodes.forEach(function (node, idx) {
            var cfg = KIND_CONFIG[node.kind] || DEFAULT_CONFIG;
            var accent = cfg.accent;
            var cx = MINI_PAD + idx * MINI_GAP + MINI_R;
            var cy = MINI_PAD + MINI_R;

            var nodeG = svgEl('g', { class: 'flow-mini-dot' });

            if (cfg.shape === 'diamond') {
                var ds = MINI_R * 0.9;
                nodeG.appendChild(svgEl('polygon', {
                    points: cx + ',' + (cy - ds) + ' ' + (cx + ds) + ',' + cy + ' ' + cx + ',' + (cy + ds) + ' ' + (cx - ds) + ',' + cy,
                    fill: accent,
                }));
            } else {
                nodeG.appendChild(svgEl('circle', {
                    cx: cx, cy: cy, r: MINI_R, fill: accent,
                }));
            }

            // Native SVG tooltip
            var title = svgEl('title');
            var tip = node.kind.charAt(0).toUpperCase() + node.kind.slice(1) + ': ' + node.label;
            if (node.meta) tip += ' — ' + node.meta;
            title.textContent = tip;
            nodeG.appendChild(title);

            svg.appendChild(nodeG);
        });

        container.appendChild(svg);
    }

    // ─── Public API ───────────────────────────────────────────────

    window.HomunFlow = {
        renderFlow: renderFlow,
        renderFlowMini: renderFlowMini,
    };

})();
