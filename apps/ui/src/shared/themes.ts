/**
 * Colour themes besides Ahab's own (Settings → Appearance): each a few named
 * colours per light and dark variant, from which every token the app, the
 * editor's syntax and the terminal read is made. Ahab's own theme is the
 * stylesheet's (globals.css) and sets nothing here. Pure: no DOM.
 */

/** The colours a theme variant names; the rest is derived from them. */
export interface ThemeColors {
	/** The page. */
	background: string;
	/** Cards, sidebars: a step from the page. */
	surface: string;
	/** Menus and popovers. */
	raised: string;
	/** Rows under the pointer, chosen rows. */
	hover: string;
	border: string;
	/** Fields' borders. */
	input: string;
	foreground: string;
	/** Secondary text. */
	muted: string;
	heading: string;
	primary: string;
	/** Text on `primary`. */
	onPrimary: string;
	red: string;
	orange: string;
	yellow: string;
	green: string;
	cyan: string;
	blue: string;
	purple: string;
	comment: string;
	/** Syntax colours that differ from what the named colours give. */
	syntax?: Partial<Record<SyntaxToken, string>>;
}

type SyntaxToken =
	| "keyword"
	| "control"
	| "string"
	| "function"
	| "type"
	| "number"
	| "constant"
	| "property"
	| "tag"
	| "parameter";

export interface ColorTheme {
	id: string;
	name: string;
	light?: ThemeColors;
	dark?: ThemeColors;
}

/** The variant shown for `dark`: the one asked for, else the one there is. */
export function variantOf(
	theme: ColorTheme,
	dark: boolean,
): { colors: ThemeColors; dark: boolean } {
	const asked = dark ? theme.dark : theme.light;
	if (asked) return { colors: asked, dark };
	const other = (dark ? theme.light : theme.dark) as ThemeColors;
	return { colors: other, dark: !dark };
}

/** Every CSS variable globals.css defines, for one variant. */
export function themeVariables(colors: ThemeColors, dark: boolean): Record<string, string> {
	const syntax = colors.syntax ?? {};
	const onColor = dark ? colors.background : "#ffffff";
	return {
		"--background": colors.background,
		"--foreground": colors.foreground,
		"--heading": colors.heading,
		"--card": colors.surface,
		"--card-foreground": colors.foreground,
		"--popover": colors.raised,
		"--popover-foreground": colors.foreground,
		"--primary": colors.primary,
		"--primary-foreground": colors.onPrimary,
		"--secondary": colors.hover,
		"--secondary-foreground": colors.foreground,
		"--muted": colors.surface,
		"--muted-foreground": colors.muted,
		"--accent": colors.hover,
		"--accent-foreground": colors.heading,
		"--destructive": colors.red,
		"--destructive-foreground": onColor,
		"--border": colors.border,
		"--input": colors.input,
		"--ring": colors.primary,
		"--success": colors.green,
		"--success-foreground": onColor,
		"--warning": colors.yellow,
		"--warning-foreground": colors.yellow,
		"--info": colors.blue,
		"--syntax-plain": colors.foreground,
		"--syntax-comment": colors.comment,
		"--syntax-keyword": syntax.keyword ?? colors.purple,
		"--syntax-control": syntax.control ?? syntax.keyword ?? colors.purple,
		"--syntax-string": syntax.string ?? colors.green,
		"--syntax-regexp": colors.red,
		"--syntax-escape": colors.orange,
		"--syntax-constant": syntax.constant ?? colors.orange,
		"--syntax-number": syntax.number ?? colors.orange,
		"--syntax-function": syntax.function ?? colors.blue,
		"--syntax-macro": syntax.function ?? colors.blue,
		"--syntax-type": syntax.type ?? colors.cyan,
		"--syntax-parameter": syntax.parameter ?? colors.foreground,
		"--syntax-property": syntax.property ?? colors.foreground,
		"--syntax-attribute": colors.yellow,
		"--syntax-tag": syntax.tag ?? colors.red,
		"--syntax-label": colors.purple,
		"--syntax-operator": colors.muted,
		"--syntax-punctuation": colors.muted,
		"--syntax-heading": colors.heading,
		"--syntax-link": colors.blue,
		// The terminal's eight colours (bright ones the same).
		"--ansi-black": dark ? colors.hover : colors.foreground,
		"--ansi-red": colors.red,
		"--ansi-green": colors.green,
		"--ansi-yellow": colors.yellow,
		"--ansi-blue": colors.blue,
		"--ansi-magenta": colors.purple,
		"--ansi-cyan": colors.cyan,
		"--ansi-white": dark ? colors.foreground : colors.hover,
	};
}

/** The variables a theme sets, to take them away again. */
export const THEME_VARIABLES = Object.keys(
	themeVariables(
		{
			background: "",
			surface: "",
			raised: "",
			hover: "",
			border: "",
			input: "",
			foreground: "",
			muted: "",
			heading: "",
			primary: "",
			onPrimary: "",
			red: "",
			orange: "",
			yellow: "",
			green: "",
			cyan: "",
			blue: "",
			purple: "",
			comment: "",
		},
		true,
	),
);

/** The themes built in, by their authors' published palettes. */
export const COLOR_THEMES: readonly ColorTheme[] = [
	{
		id: "catppuccin",
		name: "Catppuccin",
		light: {
			background: "#eff1f5",
			surface: "#e6e9ef",
			raised: "#ffffff",
			hover: "#dce0e8",
			border: "#ccd0da",
			input: "#bcc0cc",
			foreground: "#4c4f69",
			muted: "#6c6f85",
			heading: "#4c4f69",
			primary: "#8839ef",
			onPrimary: "#ffffff",
			red: "#d20f39",
			orange: "#fe640b",
			yellow: "#df8e1d",
			green: "#40a02b",
			cyan: "#179299",
			blue: "#1e66f5",
			purple: "#8839ef",
			comment: "#8c8fa1",
		},
		dark: {
			background: "#1e1e2e",
			surface: "#181825",
			raised: "#313244",
			hover: "#45475a",
			border: "#313244",
			input: "#45475a",
			foreground: "#cdd6f4",
			muted: "#a6adc8",
			heading: "#cdd6f4",
			primary: "#cba6f7",
			onPrimary: "#1e1e2e",
			red: "#f38ba8",
			orange: "#fab387",
			yellow: "#f9e2af",
			green: "#a6e3a1",
			cyan: "#94e2d5",
			blue: "#89b4fa",
			purple: "#cba6f7",
			comment: "#7f849c",
		},
	},
	{
		id: "nord",
		name: "Nord",
		dark: {
			background: "#2e3440",
			surface: "#3b4252",
			raised: "#3b4252",
			hover: "#434c5e",
			border: "#434c5e",
			input: "#4c566a",
			foreground: "#d8dee9",
			muted: "#a5adbb",
			heading: "#eceff4",
			primary: "#88c0d0",
			onPrimary: "#2e3440",
			red: "#bf616a",
			orange: "#d08770",
			yellow: "#ebcb8b",
			green: "#a3be8c",
			cyan: "#8fbcbb",
			blue: "#81a1c1",
			purple: "#b48ead",
			comment: "#616e88",
			syntax: { function: "#88c0d0", type: "#8fbcbb", keyword: "#81a1c1" },
		},
	},
	{
		id: "gruvbox",
		name: "Gruvbox",
		light: {
			background: "#fbf1c7",
			surface: "#f2e5bc",
			raised: "#f9f5d7",
			hover: "#ebdbb2",
			border: "#ebdbb2",
			input: "#d5c4a1",
			foreground: "#3c3836",
			muted: "#7c6f64",
			heading: "#282828",
			primary: "#427b58",
			onPrimary: "#fbf1c7",
			red: "#9d0006",
			orange: "#af3a03",
			yellow: "#b57614",
			green: "#79740e",
			cyan: "#427b58",
			blue: "#076678",
			purple: "#8f3f71",
			comment: "#928374",
			syntax: { keyword: "#9d0006", type: "#b57614", function: "#79740e", string: "#79740e" },
		},
		dark: {
			background: "#282828",
			surface: "#32302f",
			raised: "#3c3836",
			hover: "#504945",
			border: "#3c3836",
			input: "#504945",
			foreground: "#ebdbb2",
			muted: "#a89984",
			heading: "#fbf1c7",
			primary: "#fabd2f",
			onPrimary: "#282828",
			red: "#fb4934",
			orange: "#fe8019",
			yellow: "#fabd2f",
			green: "#b8bb26",
			cyan: "#8ec07c",
			blue: "#83a598",
			purple: "#d3869b",
			comment: "#928374",
			syntax: { keyword: "#fb4934", type: "#fabd2f", function: "#b8bb26" },
		},
	},
	{
		id: "solarized",
		name: "Solarized",
		light: {
			background: "#fdf6e3",
			surface: "#eee8d5",
			raised: "#fffbf0",
			hover: "#eee8d5",
			border: "#e6dfc8",
			input: "#d3cbb7",
			foreground: "#586e75",
			muted: "#657b83",
			heading: "#073642",
			primary: "#268bd2",
			onPrimary: "#fdf6e3",
			red: "#dc322f",
			orange: "#cb4b16",
			yellow: "#b58900",
			green: "#859900",
			cyan: "#2aa198",
			blue: "#268bd2",
			purple: "#6c71c4",
			comment: "#93a1a1",
			syntax: { keyword: "#859900", string: "#2aa198", type: "#b58900", constant: "#cb4b16" },
		},
		dark: {
			background: "#002b36",
			surface: "#073642",
			raised: "#073642",
			hover: "#0d4553",
			border: "#0d4553",
			input: "#2a5763",
			foreground: "#93a1a1",
			muted: "#839496",
			heading: "#eee8d5",
			primary: "#268bd2",
			onPrimary: "#fdf6e3",
			red: "#dc322f",
			orange: "#cb4b16",
			yellow: "#b58900",
			green: "#859900",
			cyan: "#2aa198",
			blue: "#268bd2",
			purple: "#6c71c4",
			comment: "#586e75",
			syntax: { keyword: "#859900", string: "#2aa198", type: "#b58900", constant: "#cb4b16" },
		},
	},
	{
		id: "tokyo-night",
		name: "Tokyo Night",
		light: {
			background: "#e1e2e7",
			surface: "#d5d6db",
			raised: "#e9e9ed",
			hover: "#c4c8da",
			border: "#c4c8da",
			input: "#a8aecb",
			foreground: "#3760bf",
			muted: "#6172b0",
			heading: "#3760bf",
			primary: "#2e7de9",
			onPrimary: "#ffffff",
			red: "#f52a65",
			orange: "#b15c00",
			yellow: "#8c6c3e",
			green: "#587539",
			cyan: "#007197",
			blue: "#2e7de9",
			purple: "#9854f1",
			comment: "#848cb5",
		},
		dark: {
			background: "#1a1b26",
			surface: "#16161e",
			raised: "#1f2335",
			hover: "#292e42",
			border: "#292e42",
			input: "#3b4261",
			foreground: "#c0caf5",
			muted: "#787c99",
			heading: "#c0caf5",
			primary: "#7aa2f7",
			onPrimary: "#1a1b26",
			red: "#f7768e",
			orange: "#ff9e64",
			yellow: "#e0af68",
			green: "#9ece6a",
			cyan: "#7dcfff",
			blue: "#7aa2f7",
			purple: "#bb9af7",
			comment: "#565f89",
			syntax: { type: "#2ac3de", property: "#73daca" },
		},
	},
	{
		id: "dracula",
		name: "Dracula",
		dark: {
			background: "#282a36",
			surface: "#21222c",
			raised: "#343746",
			hover: "#44475a",
			border: "#44475a",
			input: "#565972",
			foreground: "#f8f8f2",
			muted: "#a7abbe",
			heading: "#ffffff",
			primary: "#bd93f9",
			onPrimary: "#282a36",
			red: "#ff5555",
			orange: "#ffb86c",
			yellow: "#f1fa8c",
			green: "#50fa7b",
			cyan: "#8be9fd",
			blue: "#6272a4",
			purple: "#bd93f9",
			comment: "#6272a4",
			syntax: {
				keyword: "#ff79c6",
				string: "#f1fa8c",
				function: "#50fa7b",
				type: "#8be9fd",
				number: "#bd93f9",
				constant: "#bd93f9",
				parameter: "#ffb86c",
			},
		},
	},
	{
		id: "github",
		name: "GitHub",
		light: {
			background: "#ffffff",
			surface: "#f6f8fa",
			raised: "#ffffff",
			hover: "#eff2f5",
			border: "#d0d7de",
			input: "#d0d7de",
			foreground: "#1f2328",
			muted: "#59636e",
			heading: "#1f2328",
			primary: "#0969da",
			onPrimary: "#ffffff",
			red: "#cf222e",
			orange: "#bc4c00",
			yellow: "#9a6700",
			green: "#1a7f37",
			cyan: "#1b7c83",
			blue: "#0969da",
			purple: "#8250df",
			comment: "#6e7781",
			syntax: {
				keyword: "#cf222e",
				string: "#0a3069",
				function: "#8250df",
				type: "#953800",
				number: "#0550ae",
				constant: "#0550ae",
				property: "#0550ae",
				tag: "#116329",
			},
		},
		dark: {
			background: "#0d1117",
			surface: "#151b23",
			raised: "#161b22",
			hover: "#21262d",
			border: "#30363d",
			input: "#30363d",
			foreground: "#e6edf3",
			muted: "#9198a1",
			heading: "#f0f6fc",
			primary: "#4493f8",
			onPrimary: "#0d1117",
			red: "#f85149",
			orange: "#db6d28",
			yellow: "#d29922",
			green: "#3fb950",
			cyan: "#39c5cf",
			blue: "#58a6ff",
			purple: "#bc8cff",
			comment: "#8b949e",
			syntax: {
				keyword: "#ff7b72",
				string: "#a5d6ff",
				function: "#d2a8ff",
				type: "#ffa657",
				number: "#79c0ff",
				constant: "#79c0ff",
				property: "#79c0ff",
				tag: "#7ee787",
			},
		},
	},
	{
		id: "rose-pine",
		name: "Rosé Pine",
		light: {
			background: "#faf4ed",
			surface: "#fffaf3",
			raised: "#fffaf3",
			hover: "#f2e9e1",
			border: "#dfdad9",
			input: "#cecacd",
			foreground: "#575279",
			muted: "#797593",
			heading: "#575279",
			primary: "#286983",
			onPrimary: "#faf4ed",
			red: "#b4637a",
			orange: "#d7827e",
			yellow: "#ea9d34",
			green: "#56949f",
			cyan: "#56949f",
			blue: "#286983",
			purple: "#907aa9",
			comment: "#9893a5",
			syntax: { keyword: "#286983", string: "#ea9d34", function: "#d7827e", type: "#56949f" },
		},
		dark: {
			background: "#191724",
			surface: "#1f1d2e",
			raised: "#26233a",
			hover: "#403d52",
			border: "#403d52",
			input: "#524f67",
			foreground: "#e0def4",
			muted: "#908caa",
			heading: "#e0def4",
			primary: "#c4a7e7",
			onPrimary: "#191724",
			red: "#eb6f92",
			orange: "#ebbcba",
			yellow: "#f6c177",
			green: "#9ccfd8",
			cyan: "#9ccfd8",
			blue: "#31748f",
			purple: "#c4a7e7",
			comment: "#6e6a86",
			syntax: { keyword: "#31748f", string: "#f6c177", function: "#ebbcba", type: "#9ccfd8" },
		},
	},
];

/** A built-in theme by its ID; none for Ahab's own (or one no longer here). */
export function colorTheme(id: string | undefined): ColorTheme | undefined {
	return COLOR_THEMES.find((theme) => theme.id === id);
}

/** Fonts offered for the interface and for code, besides any typed by name. */
const FONT_CHOICES = {
	ui: [
		{ id: "", stack: "" },
		{ id: "system", stack: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif' },
	],
	code: [
		{ id: "", stack: "" },
		{
			id: "system",
			stack: 'ui-monospace, "SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace',
		},
	],
} as const;

/** A font family as CSS: a chosen one first, then the usual fallbacks. */
export function fontStack(chosen: string, kind: "ui" | "code"): string {
	const found = FONT_CHOICES[kind].find((choice) => choice.id === chosen);
	if (found) return found.stack;
	const name = chosen.trim().replaceAll('"', "");
	if (!name) return "";
	return kind === "ui"
		? `"${name}", system-ui, sans-serif`
		: `"${name}", "JetBrains Mono Variable", ui-monospace, monospace`;
}
