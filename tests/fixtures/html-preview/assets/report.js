document.getElementById("resources").textContent = "All loaded";
let count = 0;
document.getElementById("counter").addEventListener("click", () => {
  document.getElementById("count").textContent = String(++count);
});
document.documentElement.dataset.acceptanceReady = "true";
