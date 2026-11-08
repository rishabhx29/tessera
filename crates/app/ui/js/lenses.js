// The four questions the window can ask. `all` is the neutral start state;
// the other three are the user's own questions, in the order they were asked.
//
// A lens is not a filter. It chooses the kinds and the arms on the engine side
// (SearchMode), so the label, the placeholder and the question all describe
// the same answer.

export const LENSES = Object.freeze([
  Object.freeze({
    id: "all",
    label: "all",
    aria: "Everything, with reasons",
    placeholder: "search your screenshots, pictures and documents",
    kinds: ["image"],
  }),
  Object.freeze({
    id: "pictures",
    label: "pictures",
    aria: "Pictures, by what they look like",
    placeholder: "describe a picture, a bridge at dusk",
    kinds: ["image"],
  }),
  Object.freeze({
    id: "words",
    label: "words",
    aria: "Words inside pictures",
    placeholder: "words in a picture, the invoice total",
    kinds: ["image"],
  }),
  Object.freeze({
    id: "documents",
    label: "documents",
    aria: "Words inside documents",
    placeholder: "words in a document, the renewal date",
    kinds: ["pdf", "docx", "text"],
  }),
]);

export const DEFAULT_LENS = "all";

export function lensFor(id) {
  return LENSES.find((lens) => lens.id === id) ?? LENSES[0];
}
