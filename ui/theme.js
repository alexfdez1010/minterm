// Read the same tokens as the workspace once, before constructing any terminals.
const styles = getComputedStyle(document.documentElement);
const token = (name) => styles.getPropertyValue("--" + name).trim();

export const terminalFont = token("mono");
export const defaultFontSize = Number.parseFloat(token("type-terminal"));
export const theme = {
  background: token("canvas"),
  foreground: token("term-fg"),
  cursor: token("mint"),
  cursorAccent: token("canvas"),
  selectionBackground: token("term-selection"),
};
for (const color of ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]) {
  theme[color] = token("ansi-" + color);
  theme["bright" + color[0].toUpperCase() + color.slice(1)] = token("ansi-bright-" + color);
}
