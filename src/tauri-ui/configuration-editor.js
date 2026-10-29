/* Read-only file viewer. Formatting changes presentation only; original text stays intact. */
(() => {
  function node(tag, value, className) {
    const element = document.createElement(tag);
    element.textContent = value;
    if (className) element.className = className;
    return element;
  }
  function language(path) {
    if (/\.jsonc?$/i.test(path)) return /\.jsonc$/i.test(path) ? 'JSONC' : 'JSON';
    if (/\.toml$/i.test(path)) return 'TOML';
    if (/\.ya?ml$/i.test(path)) return 'YAML';
    return 'TEXT';
  }
  function highlight(code, source, kind) {
    code.replaceChildren();
    // Keep every character, including line endings; insert file text only as text nodes.
    const tokens = /"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|\/\/[^\r\n]*|\/\*[\s\S]*?\*\/|#[^\r\n]*|\b(?:true|false|null)\b|-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/g;
    let end = 0;
    for (const match of source.matchAll(tokens)) {
      const value = match[0];
      code.append(document.createTextNode(source.slice(end, match.index)));
      let type;
      if (/^(?:#|\/\/|\/\*)/.test(value)) type = 'comment';
      else if (/^["']/.test(value)) type = /^\s*[:=]/.test(source.slice(match.index + value.length)) ? 'key' : 'string';
      else type = /^(true|false|null)$/.test(value) ? 'literal' : 'number';
      code.append(kind === 'TEXT' ? document.createTextNode(value) : node('span', value, `config-token-${type}`));
      end = match.index + value.length;
    }
    code.append(document.createTextNode(source.slice(end)));
  }
  function formatJson(source) {
    JSON.parse(source); // Validate only. Preserve number lexemes, duplicate keys and escaped strings.
    const tokens = source.match(/"(?:\\[\s\S]|[^"\\])*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\],:]/g);
    let result = '', depth = 0;
    const newline = () => '\n' + '  '.repeat(depth);
    tokens.forEach((token, index) => {
      if (token === '{' || token === '[') {
        result += token; depth += 1;
        if (tokens[index + 1] !== (token === '{' ? '}' : ']')) result += newline();
      } else if (token === '}' || token === ']') {
        depth -= 1;
        if (tokens[index - 1] !== (token === '}' ? '{' : '[')) result += newline();
        result += token;
      } else if (token === ',') result += ',' + newline();
      else result += token === ':' ? ': ' : token;
    });
    return result + (/\n$/.test(source) ? '\n' : '');
  }
  function create(path, original) {
    const kind = language(path);
    const editor = node('div', '', 'configuration-editor');
    const viewport = node('div', '', 'configuration-editor-viewport');
    viewport.tabIndex = 0; viewport.setAttribute('role', 'region');
    viewport.setAttribute('aria-label', `${path} 文件内容（只读）`);
    const gutter = node('pre', '', 'configuration-editor-gutter'); gutter.setAttribute('aria-hidden', 'true');
    const content = node('pre', '', 'tool-configuration-content');
    const code = node('code', ''); content.append(code); viewport.append(gutter, content);
    const toolbar = node('div', '', 'configuration-editor-toolbar');
    const format = node('button', '', 'quiet configuration-format'); format.type = 'button';
    const icon = node('span', '✧', 'configuration-format-icon'); icon.setAttribute('aria-hidden', 'true');
    format.append(icon, node('span', '格式化'));
    const reset = node('button', '原文', 'quiet configuration-original'); reset.type = 'button'; reset.hidden = true;
    const info = node('span', '', 'configuration-editor-info');
    info.setAttribute('role', 'status');
    function render(source) {
      const lineCount = source.split(/\r\n|\r|\n/).length;
      gutter.textContent = Array.from({length:lineCount}, (_, index) => index + 1).join('\n');
      highlight(code, source, kind);
      info.textContent = `${kind} · ${lineCount} 行 · 只读`;
      viewport.scrollTop = 0; viewport.scrollLeft = 0;
    }
    let formatted;
    if (kind === 'JSON' || kind === 'JSONC') {
      try { formatted = formatJson(original); }
      catch { format.title = '当前内容无法按 JSON 格式化，已保留原文'; }
    } else format.title = '此文件按原格式展示';
    format.disabled = formatted === undefined;
    format.onclick = () => { render(formatted); reset.hidden = false; format.disabled = true; };
    reset.onclick = () => { render(original); reset.hidden = true; format.disabled = false; };
    toolbar.append(format, reset, info); editor.append(viewport, toolbar); render(original);
    return editor;
  }
  window.TokenMarketConfigurationEditor = {create};
})();
