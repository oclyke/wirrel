const blocks = document.querySelectorAll(".copyable");

// Without a clipboard (an insecure origin, an old browser) the blocks stay
// plain text rather than offering a button that can't work.
if (blocks.length && !navigator.clipboard) {
  console.warn(
    "wirrel: navigator.clipboard is unavailable, so no copy buttons. " +
      "Browsers expose it only on secure origins: https, localhost, 127.0.0.1, [::1].",
  );
}

if (navigator.clipboard) {
  for (const block of blocks) {
    const button = document.createElement("button");
    button.type = "button";
    button.title = "copy";
    button.setAttribute("aria-label", "copy");
    button.addEventListener("click", async () => {
      // The fence's closing newline would make a pasted command run at once.
      const text = block.querySelector("code").textContent.replace(/\n$/, "");
      try {
        await navigator.clipboard.writeText(text);
        button.dataset.state = "copied";
      } catch {
        button.dataset.state = "failed";
      }
      setTimeout(() => delete button.dataset.state, 1500);
    });
    block.append(button);
  }
}
