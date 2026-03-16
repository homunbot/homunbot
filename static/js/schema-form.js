/**
 * SchemaForm — Generates guided form fields from JSON Schema.
 *
 * Used by the automations builder to replace raw JSON textareas
 * with individual typed inputs for each tool parameter.
 *
 * Type mapping:
 *   enum           → <select> with options
 *   boolean        → <input type="checkbox">
 *   number/integer → <input type="number">
 *   string         → <input type="text">
 *
 * Fields use data-field="arg__paramName" convention so the
 * inspector change handler can route values to node.data.arguments.
 */
window.SchemaForm = {

    /**
     * Render form fields from a JSON Schema into a container element.
     *
     * @param {HTMLElement} container - Where to append generated fields
     * @param {Object} schema - Tool JSON Schema ({properties, required, type: "object"})
     * @param {Object} currentValues - Current argument values (parsed object)
     * @param {Object} [overrides] - Optional { paramName: [{value, label}] } to force
     *   specific params as <select> with pre-fetched options from API
     * @returns {boolean} true if fields were generated, false if fallback needed
     */
    render(container, schema, currentValues, overrides) {
        if (!schema || !schema.properties || typeof schema.properties !== 'object') {
            return false;
        }

        const props = schema.properties;
        const keys = Object.keys(props);
        if (keys.length === 0) return false;

        const required = Array.isArray(schema.required) ? schema.required : [];
        const values = currentValues || {};

        for (const paramName of keys) {
            const paramSchema = props[paramName];
            const isRequired = required.includes(paramName);
            const currentVal = values[paramName];

            const group = document.createElement('div');
            group.className = 'form-group';

            // Label with required indicator
            const label = document.createElement('label');
            label.textContent = this._formatLabel(paramName);
            if (isRequired) {
                const star = document.createElement('span');
                star.className = 'schema-required';
                star.textContent = '*';
                label.appendChild(star);
            }
            group.appendChild(label);

            // Create the appropriate input element
            const fieldName = 'arg__' + paramName;
            const overrideOpts = overrides && overrides[paramName];

            if (Array.isArray(overrideOpts) && overrideOpts.length > 0) {
                // Override → select with pre-fetched options from API
                const sel = document.createElement('select');
                sel.className = 'input';
                sel.dataset.field = fieldName;

                const emptyOpt = document.createElement('option');
                emptyOpt.value = '';
                emptyOpt.textContent = '-- Select --';
                sel.appendChild(emptyOpt);

                for (const item of overrideOpts) {
                    const opt = document.createElement('option');
                    opt.value = item.value;
                    opt.textContent = item.label || item.value;
                    if (currentVal !== undefined && String(currentVal) === String(item.value)) {
                        opt.selected = true;
                    }
                    sel.appendChild(opt);
                }
                group.appendChild(sel);

            } else if (Array.isArray(paramSchema.enum) && paramSchema.enum.length > 0) {
                // Enum → select dropdown
                const sel = document.createElement('select');
                sel.className = 'input';
                sel.dataset.field = fieldName;

                const emptyOpt = document.createElement('option');
                emptyOpt.value = '';
                emptyOpt.textContent = '-- Select --';
                sel.appendChild(emptyOpt);

                for (const val of paramSchema.enum) {
                    const opt = document.createElement('option');
                    opt.value = String(val);
                    opt.textContent = String(val);
                    if (currentVal !== undefined && String(currentVal) === String(val)) {
                        opt.selected = true;
                    }
                    sel.appendChild(opt);
                }
                group.appendChild(sel);

            } else if (paramSchema.type === 'boolean') {
                // Boolean → checkbox
                const checkLabel = document.createElement('label');
                checkLabel.className = 'checkbox-label';
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.dataset.field = fieldName;
                cb.checked = currentVal === true;
                checkLabel.appendChild(cb);
                checkLabel.appendChild(document.createTextNode(' Enabled'));
                group.appendChild(checkLabel);

            } else if (paramSchema.type === 'number' || paramSchema.type === 'integer') {
                // Number → number input
                const inp = document.createElement('input');
                inp.type = 'number';
                inp.className = 'input';
                inp.dataset.field = fieldName;
                if (paramSchema.type === 'integer') inp.step = '1';
                if (paramSchema.minimum !== undefined) inp.min = String(paramSchema.minimum);
                if (paramSchema.maximum !== undefined) inp.max = String(paramSchema.maximum);
                inp.value = currentVal !== undefined ? String(currentVal) : '';
                inp.placeholder = paramSchema.type === 'integer' ? '0' : '0.0';
                group.appendChild(inp);

            } else {
                // String (default) → text input
                const inp = document.createElement('input');
                inp.type = 'text';
                inp.className = 'input';
                inp.dataset.field = fieldName;
                inp.value = currentVal !== undefined ? String(currentVal) : '';
                inp.placeholder = paramSchema.description
                    ? paramSchema.description.substring(0, 60)
                    : paramName;
                group.appendChild(inp);
            }

            // Description hint
            if (paramSchema.description) {
                const hint = document.createElement('p');
                hint.className = 'schema-field-hint';
                hint.textContent = paramSchema.description;
                group.appendChild(hint);
            }

            // Attach real-time validation (AUTO-2)
            if (window.AutoValidate) {
                const fieldEl = group.querySelector('input, select, textarea');
                if (fieldEl) {
                    const rule = window.AutoValidate.fieldRule(null, 'arg__' + paramName, {
                        required: isRequired,
                        type: paramSchema.type,
                        minimum: paramSchema.minimum,
                        maximum: paramSchema.maximum,
                    });
                    if (rule) {
                        const runValidation = () => window.AutoValidate.validateField(fieldEl, rule);
                        fieldEl.addEventListener('blur', runValidation);
                        if (fieldEl.tagName === 'SELECT') {
                            fieldEl.addEventListener('change', runValidation);
                        }
                    }
                }
            }

            container.appendChild(group);
        }

        return true;
    },

    /**
     * Parse arguments from node data, handling both string and object formats.
     * @param {*} raw - node.data.arguments (string, object, or undefined)
     * @returns {Object|null} Parsed object, or null if string is unparseable
     */
    parseArguments(raw) {
        if (!raw) return {};
        if (typeof raw === 'object') return raw;
        if (typeof raw === 'string') {
            const trimmed = raw.trim();
            if (!trimmed) return {};
            try { return JSON.parse(trimmed); } catch (_) { return null; }
        }
        return {};
    },

    /**
     * Serialize arguments object to JSON string for nodeToInstruction.
     * @param {*} args - Object or string
     * @returns {string} JSON string, or empty string if no args
     */
    serializeArguments(args) {
        if (!args) return '';
        if (typeof args === 'string') return args;
        if (typeof args === 'object') {
            // Filter out empty values
            const filtered = {};
            let hasValues = false;
            for (const [k, v] of Object.entries(args)) {
                if (v !== '' && v !== undefined && v !== null) {
                    filtered[k] = v;
                    hasValues = true;
                }
            }
            return hasValues ? JSON.stringify(filtered) : '';
        }
        return '';
    },

    /**
     * Convert param name to human-readable label.
     * "max_body_chars" → "Max body chars"
     * @param {string} name
     * @returns {string}
     */
    _formatLabel(name) {
        return name
            .replace(/_/g, ' ')
            .replace(/\b\w/, c => c.toUpperCase());
    },
};
