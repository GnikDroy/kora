export function registerKoraLanguage(monaco) {
  monaco.languages.register({ id: "kora" });

  monaco.languages.setMonarchTokensProvider("kora", {
    keywords: [
      "return", "let", "if", "else", "while", "for", "break",
      "continue", "extern", "as", "struct", "new", "true", "false",
    ],
    typeKeywords: ["void", "int", "real", "char", "bool", "string"],
    operators: [
      "=", "==", "!=", "&&", "||", ">", "<", ">=", "<=",
      "+", "-", "*", "/", "%", "!",
    ],
    symbols: /[=><!&|+\-*\/%]+/,
    escapes: /\\(?:[nrt0\\'"])/,
    tokenizer: {
      root: [
        [/[a-zA-Z_]\w*(?=\s*\()/, {
          cases: {
            "@typeKeywords": "type",
            "@keywords": "keyword",
            "@default": "function",
          },
        }],
        [/[a-zA-Z_]\w*/, {
          cases: {
            "@typeKeywords": "type",
            "@keywords": "keyword",
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
      { token: "comment", foreground: "A0A1A7", fontStyle: "italic" },
      { token: "keyword", foreground: "A626A4" },
      { token: "type", foreground: "C18401" },
      { token: "string", foreground: "50A14F" },
      { token: "string.quote", foreground: "50A14F" },
      { token: "string.escape", foreground: "0184BC" },
      { token: "string.invalid", foreground: "E45649" },
      { token: "number", foreground: "986801" },
      { token: "number.float", foreground: "986801" },
      { token: "operator", foreground: "0184BC" },
      { token: "function", foreground: "4078F2" },
      { token: "identifier", foreground: "E45649" },
      { token: "delimiter", foreground: "383A42" },
      { token: "delimiter.bracket", foreground: "B58900" },
    ],
    colors: {
      "editor.background": "#FAFAFA",
      "editor.foreground": "#383A42",
      "editor.lineHighlightBackground": "#F0F0F1",
      "editor.selectionBackground": "#E5E5E6",
      "editorLineNumber.foreground": "#9D9D9F",
      "editorLineNumber.activeForeground": "#383A42",
      "editorCursor.foreground": "#526FFF",
      "editorIndentGuide.background": "#E8E8E9",
    },
  });

  monaco.editor.defineTheme("kora-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "5C6370", fontStyle: "italic" },
      { token: "keyword", foreground: "C678DD" },
      { token: "type", foreground: "E5C07B" },
      { token: "string", foreground: "98C379" },
      { token: "string.quote", foreground: "98C379" },
      { token: "string.escape", foreground: "56B6C2" },
      { token: "string.invalid", foreground: "E06C75" },
      { token: "number", foreground: "D19A66" },
      { token: "number.float", foreground: "D19A66" },
      { token: "operator", foreground: "56B6C2" },
      { token: "function", foreground: "61AFEF" },
      { token: "identifier", foreground: "E06C75" },
      { token: "delimiter", foreground: "ABB2BF" },
      { token: "delimiter.bracket", foreground: "FFD700" },
    ],
    colors: {
      "editor.background": "#282C34",
      "editor.foreground": "#ABB2BF",
      "editor.lineHighlightBackground": "#2C313C",
      "editor.selectionBackground": "#3E4451",
      "editorLineNumber.foreground": "#495162",
      "editorLineNumber.activeForeground": "#ABB2BF",
      "editorCursor.foreground": "#528BFF",
      "editorIndentGuide.background": "#3B4048",
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
