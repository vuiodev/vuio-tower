const { invoke } = window.__TAURI__.core;

let serverNameEl;
let serverPortEl;
let foldersListEl;
let folders = [];

async function loadSettings() {
  try {
    const settings = await invoke("get_settings");
    serverNameEl.value = settings.name;
    serverPortEl.value = settings.port;
    folders = settings.directories || [];
    renderFolders();
  } catch (err) {
    console.error("Failed to load settings:", err);
  }
}

function renderFolders() {
  foldersListEl.innerHTML = "";
  if (folders.length === 0) {
    foldersListEl.innerHTML = '<div class="folder-item"><span class="folder-path" style="color: var(--text-secondary); font-style: italic;">No folders monitored. Click below to add.</span></div>';
    return;
  }

  folders.forEach((folder, index) => {
    const item = document.createElement("div");
    item.className = "folder-item";
    
    const pathSpan = document.createElement("span");
    pathSpan.className = "folder-path";
    pathSpan.textContent = folder;
    pathSpan.title = folder;
    
    const delBtn = document.createElement("button");
    delBtn.className = "btn-delete";
    delBtn.type = "button";
    delBtn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>';
    delBtn.addEventListener("click", () => {
      folders.splice(index, 1);
      renderFolders();
    });

    item.appendChild(pathSpan);
    item.appendChild(delBtn);
    foldersListEl.appendChild(item);
  });
}

async function addFolder() {
  try {
    const folder = await invoke("select_directory");
    if (folder && !folders.includes(folder)) {
      folders.push(folder);
      renderFolders();
    }
  } catch (err) {
    console.error("Directory picker error:", err);
  }
}

async function saveSettings(e) {
  e.preventDefault();
  const name = serverNameEl.value.trim();
  const port = parseInt(serverPortEl.value, 10);

  if (!name || isNaN(port)) return;

  try {
    await invoke("save_settings", {
      port,
      name,
      directories: folders
    });
    
    // Close the settings window upon success
    const { getCurrentWindow } = window.__TAURI__.window;
    getCurrentWindow().close();
  } catch (err) {
    console.error("Failed to save settings:", err);
    alert("Error saving settings: " + err);
  }
}

function cancelSettings() {
  const { getCurrentWindow } = window.__TAURI__.window;
  getCurrentWindow().close();
}

window.addEventListener("DOMContentLoaded", () => {
  serverNameEl = document.querySelector("#server-name");
  serverPortEl = document.querySelector("#server-port");
  foldersListEl = document.querySelector("#folders-list");

  document.querySelector("#btn-add-folder").addEventListener("click", addFolder);
  document.querySelector("#btn-cancel").addEventListener("click", cancelSettings);
  document.querySelector("#settings-form").addEventListener("submit", saveSettings);

  loadSettings();
});
