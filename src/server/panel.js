// Augment card overlay in 8x8 pixel art like the rest of the game. Opt-in with /augment panel;
// it only appears once combat ends; pick with the mouse or keys 1-3. The server keeps the player
// invulnerable while the cards are visible and hides them if combat resumes.
(() => {
  const key = window.rotmgPanelKey;
  const client = document.querySelector('#client');
  if (!key || !client) return;
  const ICONS = ["<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"1\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"7\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"7\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"7\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8c1f2a\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8c1f2a\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#8c1f2a\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#8c1f2a\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"5\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"5\" y=\"6\" width=\"1\" height=\"1\" fill=\"#25407a\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"6\" y=\"0\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"7\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"0\" y=\"4\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"1\" y=\"6\" width=\"1\" height=\"1\" fill=\"#6e3f1c\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"0\" y=\"7\" width=\"1\" height=\"1\" fill=\"#6e3f1c\"/><rect x=\"3\" y=\"7\" width=\"1\" height=\"1\" fill=\"#6e3f1c\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"1\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"2\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"5\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"6\" y=\"0\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"0\" y=\"1\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"7\" y=\"1\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"0\" y=\"2\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"7\" y=\"2\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"0\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"7\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"0\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"7\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"6\" y=\"5\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#4a8ae8\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#25407a\"/><rect x=\"5\" y=\"6\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"7\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"7\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"2\" y=\"0\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#8f97a6\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#6e3f1c\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"6\" y=\"5\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#6e3f1c\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"5\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"6\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/><rect x=\"7\" y=\"6\" width=\"1\" height=\"1\" fill=\"#c07a3a\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"2\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"0\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"7\" y=\"2\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"7\" y=\"3\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"7\" y=\"4\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"0\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"6\" y=\"5\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"7\" y=\"5\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"1\" y=\"6\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"6\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"6\" y=\"6\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"2\" y=\"7\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"3\" y=\"7\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"4\" y=\"7\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/><rect x=\"5\" y=\"7\" width=\"1\" height=\"1\" fill=\"#d83a3a\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"5\" y=\"0\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"6\" y=\"0\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"7\" y=\"1\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"7\" y=\"2\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#58d070\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/><rect x=\"0\" y=\"6\" width=\"1\" height=\"1\" fill=\"#2a7a3c\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"1\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"5\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"5\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"6\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"0\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"2\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"0\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"7\" y=\"3\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"1\" y=\"4\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"5\" y=\"4\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"6\" y=\"4\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#b070ff\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#5e2e9c\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"4\" y=\"0\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"5\" y=\"0\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"2\" y=\"5\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"1\" y=\"6\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"2\" y=\"6\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"0\" y=\"7\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"1\" y=\"1\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"5\" y=\"1\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"0\" y=\"3\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"2\" y=\"4\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"1\" y=\"5\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"5\" y=\"5\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#7fe3ff\"/></svg>", "<svg viewBox=\"0 0 8 8\" shape-rendering=\"crispEdges\"><rect x=\"3\" y=\"0\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"0\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"1\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"1\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"1\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"2\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"2\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"6\" y=\"2\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"1\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"2\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"4\" y=\"3\" width=\"1\" height=\"1\" fill=\"#ecebe4\"/><rect x=\"5\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"6\" y=\"3\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"4\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"5\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"4\" y=\"5\" width=\"1\" height=\"1\" fill=\"#f0c43c\"/><rect x=\"3\" y=\"6\" width=\"1\" height=\"1\" fill=\"#a87a1c\"/><rect x=\"4\" y=\"6\" width=\"1\" height=\"1\" fill=\"#a87a1c\"/></svg>"];
  const style = document.createElement('style');
  // Pixel frames: hard 4 px borders and stepped box-shadows, no rounding.
  style.textContent = `
    .ap-layer { position: absolute; inset: 0; display: none; align-items: center; justify-content: center; z-index: 10;
      background: repeating-linear-gradient(0deg, rgba(0,0,0,.70) 0 4px, rgba(0,0,0,.62) 4px 8px);
      font: bold 12px/1.4 "Courier New", monospace; color: #ecebe4; -webkit-font-smoothing: none; font-smooth: never;
      text-transform: uppercase; letter-spacing: 1px; image-rendering: pixelated; }
    .ap-layer.on { display: flex; }
    .ap-box { text-align: center; transform: scale(var(--ap-scale, 1)); }
    .ap-title { font-size: 20px; margin: 0 0 4px; color: #f0c43c; text-shadow: 4px 4px 0 #000; }
    .ap-sub { color: #8f97a6; font-size: 11px; margin-bottom: 16px; text-shadow: 2px 2px 0 #000; }
    .ap-cards { display: flex; gap: 20px; justify-content: center; }
    .ap-card { width: 176px; min-height: 236px; padding: 12px 10px; cursor: pointer; position: relative;
      background: #1b1d26; border: 4px solid var(--c);
      box-shadow: 0 0 0 4px #000, 8px 8px 0 4px #000, inset 0 0 0 4px #262a36;
      display: flex; flex-direction: column; align-items: center; gap: 8px; }
    .ap-card:hover, .ap-card:focus-visible { transform: translateY(-8px); outline: none;
      box-shadow: 0 0 0 4px #000, 8px 16px 0 4px #000, inset 0 0 0 4px var(--c); }
    .ap-card.stat { --c: #4a8ae8; } .ap-card.weapon { --c: #f0c43c; }
    .ap-key { position: absolute; top: 6px; left: 8px; width: 20px; height: 20px; line-height: 20px;
      background: var(--c); color: #000; font-size: 13px; box-shadow: 2px 2px 0 #000; }
    .ap-type { position: absolute; top: 9px; right: 8px; font-size: 9px; color: var(--c); }
    .ap-icon { width: 64px; height: 64px; margin-top: 22px; padding: 8px; background: #0f1016; box-shadow: 0 0 0 4px #000, inset 0 0 0 4px #262a36; }
    .ap-icon svg { width: 100%; height: 100%; display: block; }
    .ap-name { font-size: 13px; margin-top: 6px; text-shadow: 2px 2px 0 #000; }
    .ap-level { font-size: 11px; color: var(--c); }
    .ap-pips { display: flex; gap: 4px; }
    .ap-pips i { width: 8px; height: 8px; background: #262a36; box-shadow: 0 0 0 2px #000; }
    .ap-pips i.on { background: var(--c); }
    .ap-text { font-size: 10px; color: #c9ccd2; text-transform: none; letter-spacing: 0; }
    .ap-foot { margin-top: 20px; font-size: 10px; color: #8f97a6; text-shadow: 2px 2px 0 #000; }
    .ap-busy .ap-card { opacity: .45; pointer-events: none; }`;
  document.head.append(style);
  const layer = document.createElement('div');
  layer.className = 'ap-layer';
  layer.setAttribute('role', 'dialog');
  layer.setAttribute('aria-label', 'Choose an augment');
  client.append(layer);
  // Cards are laid out for 800x600 and scale with the game (window or fullscreen).
  new ResizeObserver(() => layer.style.setProperty('--ap-scale', client.clientWidth / 800)).observe(client);
  const esc = s => String(s).replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
  let state = {open: false}, shown = '', sending = false;
  // After a successful POST, suppress stale cards until the server reports choosing (in-flight
  // save) or an empty offer — a bare successful GET can briefly re-publish the pre-take offer.
  let pickLock = false, sawChoosing = false;
  function render() {
    const visible = state.open && state.cards && state.cards.length > 0;
    layer.classList.toggle('on', visible);
    layer.classList.toggle('ap-busy', !!state.choosing || sending || pickLock);
    if (!visible) { shown = ''; return; }
    const signature = JSON.stringify(state.cards) + state.pending;
    if (signature === shown) return;
    shown = signature;
    layer.innerHTML = `<div class="ap-box"><p class="ap-title">Choose an augment</p>
      <div class="ap-sub">${state.pending} to choose · invulnerable while choosing</div>
      <div class="ap-cards">${state.cards.map(c => `
        <div class="ap-card ${c.weapon ? 'weapon' : 'stat'}" tabindex="0" data-slot="${c.slot}">
          <span class="ap-key">${c.slot}</span><span class="ap-type">${c.weapon ? 'Weapon' : 'Stat'}</span>
          <div class="ap-icon">${ICONS[c.augment] || ''}</div>
          <div class="ap-name">${esc(c.name)}</div><div class="ap-level">Level ${c.level}/${c.max}</div>
          <div class="ap-pips">${Array.from({length: c.max}, (_, i) => `<i class="${i < c.level ? 'on' : ''}"></i>`).join('')}</div>
          <div class="ap-text">${esc(c.text)}</div></div>`).join('')}</div>
      <div class="ap-foot">Click a card or press 1, 2 or 3</div></div>`;
  }
  async function choose(slot) {
    if (sending || !state.open || !state.cards?.some(c => c.slot === slot)) return;
    sending = true; render();
    let applied = false;
    try {
      const r = await fetch('/api/augments/panel', {method: 'POST', headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({key, slot}), signal: AbortSignal.timeout(1500)});
      applied = r.ok;
    } catch { /* timeout/network: leave cards up; poll() refreshes */ }
    finally {
      // Un-busy before poll: a soft-fail GET would skip render and leave pointer-events:none.
      sending = false;
      // Drop choosable cards once the server accepted the pick. A following soft-fail GET
      // must not keep the old offer clickable (pending>1 redeals; wrong second pick).
      if (applied) {
        pickLock = true;
        sawChoosing = false;
        state = {...state, cards: [], choosing: true};
      }
      render();
      poll();
    }
  }
  layer.addEventListener('click', e => { const card = e.target.closest('.ap-card'); if (card) choose(+card.dataset.slot); });
  // Capture phase: keys 1-3 go to the cards, not the game, only while they are visible.
  window.addEventListener('keydown', e => {
    if (!layer.classList.contains('on') || !['1', '2', '3'].includes(e.key)) return;
    e.preventDefault(); e.stopImmediatePropagation();
    choose(+e.key);
  }, true);
  // Mirror server OVERLAY_ALIVE_MS: after this without a confirmed poll, invuln is gone.
  const OVERLAY_ALIVE_MS = 2000;
  let timer = 0, generation = 0, lastBeat = 0, recoverUntil = 0;
  function leaseAlive() {
    // lastBeat is stamped at request start so soft-fail grace cannot outrun the server lease by RTT.
    return lastBeat > 0 && Date.now() - lastBeat <= OVERLAY_ALIVE_MS;
  }
  // Fast cadence only while cards are up or in the post-release recover window.
  // Do NOT key off leaseAlive: every successful idle poll would reset lastBeat and lock 400ms forever.
  function fastPoll() {
    return state.open || Date.now() < recoverUntil;
  }
  /** Soft failures keep cards briefly; once the server lease is past, drop the local overlay
   *  so clicks reach the map, but keep a short fast-retry window to recover. */
  function releaseIfStale() {
    if (!state.open || leaseAlive()) return;
    state = {open: false};
    recoverUntil = Date.now() + OVERLAY_ALIVE_MS;
    render();
  }
  function schedule() {
    clearTimeout(timer);
    // Idle polls pause while hidden. Keep the open-panel / recovery heartbeat running:
    // the server drops overlay invulnerability if panel_state is not hit within OVERLAY_ALIVE_MS.
    if (document.hidden && !fastPoll()) return;
    // Poll often while cards are up or recovering; idle below the 2s overlay heartbeat otherwise.
    timer = setTimeout(poll, fastPoll() ? 400 : 1000);
  }
  async function poll() {
    // Cancel a pending timer so resume/choose cannot overlap a scheduled tick (double GET + gen bump).
    clearTimeout(timer);
    timer = 0;
    const gen = ++generation;
    const beatAt = Date.now();
    try {
      const r = await fetch(`/api/augments/panel?key=${key}`, {
        cache: 'no-store', signal: AbortSignal.timeout(1500)});
      if (r.ok) {
        let next = await r.json();
        // Check after body parse so a slow older poll cannot overwrite a newer resume/choose.
        if (gen !== generation) return;
        lastBeat = beatAt;
        recoverUntil = 0;
        if (pickLock) {
          if (next.choosing) sawChoosing = true;
          if (sawChoosing && !next.choosing) {
            pickLock = false;
          } else if (!next.choosing && next.cards?.length) {
            // Pre-take offer still live without sticky choosing — keep it non-clickable.
            next = {...next, cards: [], choosing: true};
          } else if (!next.choosing && !next.cards?.length) {
            pickLock = false;
          }
        }
        state = next;
        render();
      } else {
        // Soft failure: keep prior state until the server lease would have expired.
        releaseIfStale();
      }
    } catch {
      releaseIfStale();
    }
    if (gen !== generation) return;
    schedule();
  }
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) {
      if (!fastPoll()) clearTimeout(timer);
      // else leave the open-panel / recovery heartbeat running
    } else {
      poll();
    }
  });
  poll();
})();
