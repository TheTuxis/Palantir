const applyTransitionVisibility = () => {
  document.querySelectorAll<HTMLElement>(".review").forEach((review) => {
    const state = review.querySelectorAll("dd")[0]?.textContent?.trim();
    const allowed = state === "Backlog" ? ["Mover a To do"] : state === "Review" ? ["Mover a Backlog", "Mover a Done"] : [];
    review.querySelectorAll<HTMLButtonElement>(".transitions button").forEach((button) => {
      button.hidden = !allowed.includes(button.textContent?.trim() ?? "");
    });
    const transitions = review.querySelector<HTMLElement>(".transitions");
    if (transitions) transitions.hidden = allowed.length === 0;
  });
};
new MutationObserver(applyTransitionVisibility).observe(document.documentElement, { childList: true, subtree: true, characterData: true });
document.addEventListener("DOMContentLoaded", applyTransitionVisibility);
