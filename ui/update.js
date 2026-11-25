const status = document.querySelector("#status");
const detail = document.querySelector("#detail");
const progress = document.querySelector("#progress");
const send = (action) => window.ipc.postMessage(JSON.stringify({ action }));
window.mintermUpdate = {
  receive(update) {
    status.textContent = update.status;
    detail.textContent = update.detail;
    detail.hidden = !update.detail;
    if (update.fraction == null) progress.removeAttribute("value");
    else progress.value = update.fraction;
  },
  fail(error) {
    status.textContent = "Could not update Minterm";
    detail.textContent = error;
    detail.hidden = false;
    progress.hidden = true;
  },
};
document.querySelector("#continue").addEventListener("click", () => send("cancelUpdate"));
document.querySelector("#drag").addEventListener("mousedown", () => send("drag"));
send("startUpdate");
