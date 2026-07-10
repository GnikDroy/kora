export function registerKoraLanguage(monaco) {
  monaco.languages.register({ id: "kora" });

  monaco.languages.setMonarchTokensProvider("kora", {
    keywords: [
      "return", "let", "if", "else", "while", "for", "break", "continue",
      "extern", "as", "struct", "impl", "new", "import", "self",
    ],
    typeKeywords: ["void", "int", "real", "char", "bool", "string"],
    constants: ["true", "false", "none"],
    operators: [
      "=", "==", "!=", "&&", "||", ">", "<", ">=", "<=", "+", "-", "*",
      "/", "%", "!", "&", "|", "^", "<<", ">>", "?",
    ],
    symbols: /[=><!&|+\-*\/%^?]+/,
    escapes: /\\(?:[nrt0\\'"])/,
    tokenizer: {
      root: [
        [/[a-zA-Z_]\w*(?=\s*\()/, {
          cases: {
            "@typeKeywords": "type",
            "@keywords": "keyword",
            "@constants": "constant",
            "@default": "function",
          },
        }],
        [/[a-zA-Z_]\w*/, {
          cases: {
            "@typeKeywords": "type",
            "@keywords": "keyword",
            "@constants": "constant",
            "@default": "identifier",
          },
        }],
        { include: "@whitespace" },
        [/[{}()\[\]]/, "@brackets"],
        [/\d+\.\d+/, "number.float"],
        [/\d+/, "number"],
        [/@symbols/, {
          cases: { "@operators": "operator", "@default": "" },
        }],
        [/"([^"\\]|\\.)*$/, "string.invalid"],
        [/"/, { token: "string.quote", next: "@string" }],
        [/'(@escapes|[^\\'])'/, "string"],
        [/'/, "string.invalid"],
        [/[;:,.]/, "delimiter"],
      ],
      string: [
        [/[^\\"]+/, "string"],
        [/@escapes/, "string.escape"],
        [/\\./, "string.escape.invalid"],
        [/"/, { token: "string.quote", next: "@pop" }],
      ],
      whitespace: [
        [/[ \t\r\n]+/, "white"],
        [/#.*$/, "comment"],
      ],
    },
  });

  monaco.editor.defineTheme("kora-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "comment", foreground: "928374", fontStyle: "italic" },
      { token: "keyword", foreground: "9D0006" },
      { token: "type", foreground: "B57614" },
      { token: "string", foreground: "79740E" },
      { token: "string.quote", foreground: "79740E" },
      { token: "string.escape", foreground: "AF3A03" },
      { token: "string.invalid", foreground: "CC241D" },
      { token: "number", foreground: "8F3F71" },
      { token: "number.float", foreground: "8F3F71" },
      { token: "constant", foreground: "8F3F71" },
      { token: "operator", foreground: "427B58" },
      { token: "function", foreground: "076678" },
      { token: "identifier", foreground: "3C3836" },
      { token: "delimiter", foreground: "3C3836" },
      { token: "delimiter.bracket", foreground: "AF3A03" },
    ],
    colors: {
      "editor.background": "#FEFAEB",
      "editor.foreground": "#3C3836",
      "editor.lineHighlightBackground": "#F8F0D4",
      "editor.selectionBackground": "#EBDBB2",
      "editorLineNumber.foreground": "#BDAE93",
      "editorLineNumber.activeForeground": "#3C3836",
      "editorCursor.foreground": "#9D0006",
      "editorIndentGuide.background": "#EBDBB2",
    },
  });

  monaco.editor.defineTheme("kora-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "928374", fontStyle: "italic" },
      { token: "keyword", foreground: "FB4934" },
      { token: "type", foreground: "FABD2D" },
      { token: "string", foreground: "B8BB26" },
      { token: "string.quote", foreground: "B8BB26" },
      { token: "string.escape", foreground: "FE8019" },
      { token: "string.invalid", foreground: "FB4934" },
      { token: "number", foreground: "D3869B" },
      { token: "number.float", foreground: "D3869B" },
      { token: "constant", foreground: "D3869B" },
      { token: "operator", foreground: "8EC07C" },
      { token: "function", foreground: "83A598" },
      { token: "identifier", foreground: "EBDBB2" },
      { token: "delimiter", foreground: "EBDBB2" },
      { token: "delimiter.bracket", foreground: "FE8019" },
    ],
    colors: {
      "editor.background": "#282828",
      "editor.foreground": "#EBDBB2",
      "editor.lineHighlightBackground": "#3C3836",
      "editor.selectionBackground": "#504945",
      "editorLineNumber.foreground": "#7C6F64",
      "editorLineNumber.activeForeground": "#EBDBB2",
      "editorCursor.foreground": "#FB4934",
      "editorIndentGuide.background": "#3C3836",
    },
  });

  monaco.languages.setLanguageConfiguration("kora", {
    comments: { lineComment: "#" },
    brackets: [["{", "}"], ["[", "]"], ["(", ")"]],
    autoClosingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    surroundingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
    ],
  });
}
