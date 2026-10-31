(async () => {
    const output = document.createElement('pre');
    output.id = 'acp2api-visual-audit-result';
    output.hidden = true;
    try {
        await document.fonts.ready;
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = 1;
        const context = canvas.getContext('2d', {willReadFrequently: true});
        const rgba = value => {
            context.clearRect(0, 0, 1, 1);
            context.fillStyle = value;
            context.fillRect(0, 0, 1, 1);
            return Array.from(context.getImageData(0, 0, 1, 1).data);
        };
        const properties = [
            'box-shadow', 'text-shadow', 'filter', 'font-family', 'content', 'display', 'visibility', 'opacity',
            'border-top-left-radius', 'border-top-right-radius',
            'border-bottom-left-radius', 'border-bottom-right-radius',
            'outline-width', 'outline-style', 'text-decoration-line',
            'border-top-width', 'border-top-style', 'border-right-width', 'border-right-style',
            'border-bottom-width', 'border-bottom-style', 'border-left-width', 'border-left-style'
        ];
        const paint = (element, pseudo = null) => {
            const style = getComputedStyle(element, pseudo);
            const css = Object.fromEntries(properties.map(name => [name, style.getPropertyValue(name)]));
            const colors = {};
            for (const name of style) {
                if (name.startsWith('--') || !(name.includes('color') || name === 'fill' || name === 'stroke')) continue;
                const value = style.getPropertyValue(name);
                if (CSS.supports('color', value)) colors[name] = rgba(value);
            }
            return {css, colors};
        };
        const visible = element => {
            const rect = element.getBoundingClientRect();
            if (rect.width === 0 || rect.height === 0) return false;
            for (let node = element; node; node = node.parentElement) {
                const style = getComputedStyle(node);
                if (style.visibility !== 'visible' || style.display === 'none' || Number(style.opacity) === 0) return false;
            }
            return true;
        };
        const textBeside = element => Array.from(element.parentNode?.childNodes ?? []).some(node => {
            if (node === element || !(node.textContent ?? '').trim()) return false;
            if (node.nodeType === Node.ELEMENT_NODE) return visible(node) && !!node.innerText?.trim();
            if (node.nodeType !== Node.TEXT_NODE) return false;
            const range = document.createRange();
            range.selectNode(node);
            return Array.from(range.getClientRects()).some(rect => rect.width > 0 && rect.height > 0);
        });
        const elements = Array.from(document.querySelectorAll('*'));
        const nodes = elements.map((element, index) => {
            const rect = element.getBoundingClientRect();
            return {
                index, tag: element.localName, role: element.getAttribute('role') ?? '',
                href: element.hasAttribute('href'), visible: visible(element),
                disabled: element.matches(':disabled') || element.getAttribute('aria-disabled') === 'true',
                rect: [rect.width, rect.height], text_beside: textBeside(element),
                paint: paint(element), before: paint(element, '::before'), after: paint(element, '::after'),
                focus: null, focus_active: false, focus_visible: false
            };
        });
        // Headless Chrome activates the page after load; before that, no element matches :focus-visible.
        while (!document.hasFocus()) await new Promise(resolve => setTimeout(resolve, 10));
        for (const node of nodes) {
            if (!node.visible || node.disabled || !['input', 'select'].includes(node.tag)) continue;
            const element = elements[node.index];
            element.focus({preventScroll: true, focusVisible: true});
            node.focus_active = document.activeElement === element;
            node.focus_visible = element.matches(':focus-visible');
            node.focus = [paint(element), paint(element, '::before'), paint(element, '::after')];
            element.blur();
        }
        const resources = document.querySelectorAll('link, script[src]').length;
        const fontFaces = [];
        let imports = 0;
        const rules = sheet => {
            for (const rule of sheet.cssRules) {
                if (rule.type === CSSRule.FONT_FACE_RULE) fontFaces.push(rule.style.fontFamily);
                if (rule.type === CSSRule.IMPORT_RULE) imports++;
                if (rule.cssRules) rules(rule);
            }
        };
        if (resources === 0) {
            for (const sheet of document.styleSheets) rules(sheet);
        }
        const audit = {
            browser: navigator.userAgent,
            dark: matchMedia('(prefers-color-scheme: dark)').matches,
            resources: resources + imports,
            font_faces: fontFaces,
            fonts: Array.from(document.fonts, font => ({family: font.family, status: font.status})),
            nodes
        };
        output.textContent = Array.from(new TextEncoder().encode(JSON.stringify(audit)), byte => byte.toString(16).padStart(2, '0')).join('');
    } catch (error) {
        output.textContent = Array.from(new TextEncoder().encode(JSON.stringify({error: String(error)})), byte => byte.toString(16).padStart(2, '0')).join('');
    }
    document.body.append(output);
})();
