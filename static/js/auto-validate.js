// Homun — Automation Builder Real-time Validation (AUTO-2)
// Provides field-level, node-level, and flow-level validation.
// Loaded before automations.js so window.AutoValidate is available at init.

window.AutoValidate = {

    // ── Field-level Validation ──────────────────────────────

    /**
     * Validate a single input element against a rule.
     * Applies/removes CSS classes and shows/hides error hints.
     * @param {HTMLElement} inputEl
     * @param {Object} rule - { required, type, min, max, customFn, message }
     * @returns {{ valid: boolean, message: string }}
     */
    validateField(inputEl, rule) {
        if (!inputEl || !rule) return { valid: true, message: '' };

        // Skip validation for async-loading selects (options still loading)
        if (inputEl.tagName === 'SELECT' && inputEl.options.length <= 1) {
            var first = inputEl.options[0];
            if (first && /loading/i.test(first.textContent)) {
                return { valid: true, message: '' };
            }
        }

        var value = inputEl.type === 'checkbox' ? inputEl.checked : inputEl.value;
        var result = this._checkRule(value, rule);

        this._applyFieldState(inputEl, result.valid, result.message);
        return result;
    },

    /**
     * Check a value against a validation rule (pure logic, no DOM).
     * @returns {{ valid: boolean, message: string }}
     */
    _checkRule(value, rule) {
        // Custom function takes priority
        if (rule.customFn) {
            return rule.customFn(value);
        }

        var strVal = String(value || '').trim();

        // Required check
        if (rule.required && strVal === '') {
            return { valid: false, message: rule.message || 'Required field' };
        }

        // Skip further checks if empty and not required
        if (strVal === '') return { valid: true, message: '' };

        // Type checks
        if (rule.type === 'number' || rule.type === 'integer') {
            var num = Number(strVal);
            if (isNaN(num)) {
                return { valid: false, message: 'Must be a number' };
            }
            if (rule.type === 'integer' && !Number.isInteger(num)) {
                return { valid: false, message: 'Must be a whole number' };
            }
            if (rule.min !== undefined && num < rule.min) {
                return { valid: false, message: 'Minimum: ' + rule.min };
            }
            if (rule.max !== undefined && num > rule.max) {
                return { valid: false, message: 'Maximum: ' + rule.max };
            }
        }

        return { valid: true, message: '' };
    },

    // ── Rule Factory ────────────────────────────────────────

    /**
     * Build a validation rule for a given node kind and field.
     * @param {string|null} nodeKind
     * @param {string} fieldName
     * @param {Object} [schemaInfo] - { required, type, minimum, maximum }
     * @returns {Object|null} rule, or null if no validation needed
     */
    fieldRule(nodeKind, fieldName, schemaInfo) {
        // SchemaForm arg__ fields: derive rule from JSON Schema info
        if (fieldName && fieldName.startsWith('arg__') && schemaInfo) {
            var rule = {};
            var hasRule = false;
            if (schemaInfo.required) { rule.required = true; hasRule = true; }
            if (schemaInfo.type === 'number' || schemaInfo.type === 'integer') {
                rule.type = schemaInfo.type;
                hasRule = true;
            }
            if (schemaInfo.minimum !== undefined) { rule.min = schemaInfo.minimum; hasRule = true; }
            if (schemaInfo.maximum !== undefined) { rule.max = schemaInfo.maximum; hasRule = true; }
            return hasRule ? rule : null;
        }

        // Cron fields
        if (nodeKind === 'trigger' && fieldName && fieldName.startsWith('cron')) {
            var cronField = { cronMinute: 'minute', cronHour: 'hour', cronDom: 'dom', cronMonth: 'month', cronDow: 'dow' }[fieldName];
            if (cronField) {
                return { customFn: function (val) { return AutoValidate.validateCronField(cronField, val); } };
            }
        }

        // Per-kind required fields
        var REQUIRED_FIELDS = {
            tool: ['tool_name'],
            skill: ['skill_name'],
            mcp: ['server', 'tool'],
            llm: ['prompt'],
            condition: ['expression'],
            deliver: ['target'],
            approve: ['approve_channel'],
            subprocess: ['workflow_ref'],
            transform: ['template'],
        };

        if (nodeKind && REQUIRED_FIELDS[nodeKind]) {
            if (REQUIRED_FIELDS[nodeKind].indexOf(fieldName) !== -1) {
                return { required: true };
            }
        }

        // Special numeric fields
        if (nodeKind === 'trigger' && fieldName === 'intervalHours') {
            return { required: true, type: 'integer', min: 1, max: 168 };
        }
        if (nodeKind === 'loop' && fieldName === 'max_iterations') {
            return { required: true, type: 'integer', min: 1, max: 100 };
        }

        return null;
    },

    // ── Cron Validation ─────────────────────────────────────

    /**
     * Validate a single cron field value.
     * @param {string} field - 'minute'|'hour'|'dom'|'month'|'dow'
     * @param {string} value
     * @returns {{ valid: boolean, message: string }}
     */
    validateCronField(field, value) {
        var ranges = { minute: [0, 59], hour: [0, 23], dom: [1, 31], month: [1, 12], dow: [0, 7] };
        var range = ranges[field];
        if (!range) return { valid: true, message: '' };

        var val = String(value || '').trim();
        if (!val) return { valid: false, message: 'Required' };

        // Split on commas for lists like "1,5,10"
        var parts = val.split(',');
        for (var i = 0; i < parts.length; i++) {
            var part = parts[i].trim();
            if (!part) return { valid: false, message: 'Invalid format' };

            // Handle step: "*/5" or "1-10/2"
            var stepParts = part.split('/');
            if (stepParts.length > 2) return { valid: false, message: 'Invalid step' };

            var base = stepParts[0];
            var step = stepParts[1];

            if (step !== undefined) {
                var stepNum = Number(step);
                if (!Number.isInteger(stepNum) || stepNum < 1) {
                    return { valid: false, message: 'Step must be a positive integer' };
                }
            }

            // Check base: "*" or "N" or "N-M"
            if (base === '*') continue;

            var dashParts = base.split('-');
            if (dashParts.length > 2) return { valid: false, message: 'Invalid range' };

            for (var j = 0; j < dashParts.length; j++) {
                var n = Number(dashParts[j]);
                if (!Number.isInteger(n)) return { valid: false, message: 'Must be a number' };
                if (n < range[0] || n > range[1]) {
                    return { valid: false, message: 'Must be ' + range[0] + '-' + range[1] };
                }
            }

            // Range order: start <= end
            if (dashParts.length === 2) {
                if (Number(dashParts[0]) > Number(dashParts[1])) {
                    return { valid: false, message: 'Range start must be \u2264 end' };
                }
            }
        }

        return { valid: true, message: '' };
    },

    // ── Node-level Validation ───────────────────────────────

    /**
     * Validate a complete node's required fields.
     * Stores result in node._errors for canvas rendering.
     * @param {Object} node - { kind, data, _errors? }
     * @returns {string[]} Array of error messages
     */
    validateNode(node) {
        if (!node) return [];
        var errors = [];
        var d = node.data || {};

        // Get fields to check based on kind
        var fieldsToCheck = this._fieldsForKind(node.kind, d);

        for (var i = 0; i < fieldsToCheck.length; i++) {
            var fc = fieldsToCheck[i];
            var rule = fc.rule;
            var value = d[fc.field];
            // For arg__ fields, look in arguments object
            if (fc.field.startsWith('arg__') && typeof d.arguments === 'object' && d.arguments) {
                value = d.arguments[fc.field.substring(5)];
            }
            var result = this._checkRule(value, rule);
            if (!result.valid) {
                errors.push((fc.label || fc.field) + ': ' + result.message);
            }
        }

        node._errors = errors;
        return errors;
    },

    /**
     * Get the list of fields to validate for a node kind.
     * Returns [{ field, label, rule }]
     */
    _fieldsForKind(kind, data) {
        var fields = [];
        var req = function (field, label) { return { field: field, label: label || field, rule: { required: true } }; };

        switch (kind) {
            case 'tool':
                fields.push(req('tool_name', 'Tool'));
                break;
            case 'skill':
                fields.push(req('skill_name', 'Skill'));
                break;
            case 'mcp':
                fields.push(req('server', 'MCP Server'));
                // Only validate tool if server is already selected
                // (tools load async after server selection)
                if (data.server) fields.push(req('tool', 'MCP Tool'));
                break;
            case 'llm':
                fields.push(req('prompt', 'Prompt'));
                break;
            case 'condition':
                fields.push(req('expression', 'Condition'));
                break;
            case 'deliver':
                fields.push(req('target', 'Target'));
                break;
            case 'approve':
                fields.push(req('approve_channel', 'Channel'));
                break;
            case 'subprocess':
                fields.push(req('workflow_ref', 'Workflow'));
                break;
            case 'transform':
                fields.push(req('template', 'Template'));
                break;
            case 'loop':
                fields.push({ field: 'max_iterations', label: 'Max iterations', rule: { required: true, type: 'integer', min: 1, max: 100 } });
                break;
            case 'trigger':
                var mode = data.mode || 'daily';
                if (mode === 'cron') {
                    var self = this;
                    var cronFields = [
                        { key: 'cronMinute', cronField: 'minute', label: 'Minute' },
                        { key: 'cronHour', cronField: 'hour', label: 'Hour' },
                        { key: 'cronDom', cronField: 'dom', label: 'Day of month' },
                        { key: 'cronMonth', cronField: 'month', label: 'Month' },
                        { key: 'cronDow', cronField: 'dow', label: 'Weekday' },
                    ];
                    cronFields.forEach(function (cf) {
                        fields.push({
                            field: cf.key, label: cf.label,
                            rule: { customFn: function (val) { return self.validateCronField(cf.cronField, val); } },
                        });
                    });
                } else if (mode === 'interval') {
                    fields.push({ field: 'intervalHours', label: 'Interval', rule: { required: true, type: 'integer', min: 1, max: 168 } });
                }
                break;
        }
        return fields;
    },

    // ── Flow-level Validation ───────────────────────────────

    /**
     * Validate overall flow structure.
     * @param {Array} nodes
     * @param {Array} edges
     * @param {string} name
     * @returns {{ valid: boolean, errors: string[] }}
     */
    validateFlow(nodes, edges, name) {
        var errors = [];
        if (!name || !name.trim()) {
            errors.push('Automation name is required');
        }
        var hasTrigger = nodes.some(function (n) { return n.kind === 'trigger'; });
        var hasProcessing = nodes.some(function (n) { return n.kind !== 'trigger' && n.kind !== 'deliver'; });
        if (!hasTrigger) errors.push('Flow needs a Trigger node');
        if (!hasProcessing) errors.push('Flow needs at least one processing node');
        return { valid: errors.length === 0, errors: errors };
    },

    /**
     * Full validation: all nodes + flow structure.
     * Marks all node._errors and returns aggregate result.
     * @returns {{ valid: boolean, errors: string[] }}
     */
    validateAll(nodes, edges, name) {
        var allErrors = [];

        // Flow-level
        var flowResult = this.validateFlow(nodes, edges, name);
        allErrors = allErrors.concat(flowResult.errors);

        // Node-level
        var self = this;
        nodes.forEach(function (node) {
            var nodeErrors = self.validateNode(node);
            nodeErrors.forEach(function (e) {
                allErrors.push(node.title + ' \u2014 ' + e);
            });
        });

        return { valid: allErrors.length === 0, errors: allErrors };
    },

    // ── DOM Helpers ─────────────────────────────────────────

    /**
     * Apply validation state to an input element.
     * Reuses existing CSS: .input-invalid, .form-hint.validation-error
     */
    _applyFieldState(inputEl, valid, message) {
        if (!inputEl) return;
        var group = inputEl.closest('.form-group');

        if (valid) {
            inputEl.classList.remove('input-invalid');
            if (group) {
                var existingHint = group.querySelector('.validation-error');
                if (existingHint) existingHint.remove();
            }
        } else {
            inputEl.classList.add('input-invalid');
            if (group && message) {
                var hint = group.querySelector('.validation-error');
                if (!hint) {
                    hint = document.createElement('p');
                    hint.className = 'form-hint validation-error';
                    group.appendChild(hint);
                }
                hint.textContent = message;
            }
        }
    },
};
