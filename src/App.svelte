<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { tick } from "svelte";
  import TabBar from "./lib/TabBar.svelte";
  import type { Tab } from "./lib/TabBar.svelte";
  import PlaceholderTab from "./lib/PlaceholderTab.svelte";
  import DrivePicker from "./lib/DrivePicker.svelte";
  import PathInput from "./lib/PathInput.svelte";
  import Scanning from "./lib/Scanning.svelte";
  import DirectoryView from "./lib/DirectoryView.svelte";
  import ConfirmDelete from "./lib/ConfirmDelete.svelte";
  import WindowControls from "./lib/WindowControls.svelte";
  import type { MarkedEntry } from "./lib/DirectoryView.svelte";

  const isWindows = navigator.userAgent.includes("Windows");

  function startDrag(e: MouseEvent) {
    if (e.buttons === 1) {
      getCurrentWindow().startDragging();
    }
  }

  type StorageView = "drive-picker" | "path-input" | "scanning" | "browser" | "confirm-delete";

  const tabLabels: Record<Tab, string> = {
    performance: "Performance",
    rgb: "RGB",
    "apps-games": "Apps & Games",
    storage: "Storage",
  };

  let activeTab: Tab = $state("storage");
  let storageView: StorageView = $state("drive-picker");
  let scanPath: string = $state("");
  let filesScanned: number = $state(0);
  let dirsScanned: number = $state(0);
  let scanErrors: number = $state(0);

  // State for confirm-delete view
  let deleteSelections: Map<string, MarkedEntry[]> = $state(new Map());
  let directoryViewRef: DirectoryView | undefined = $state();

  function onDriveSelect(path: string) {
    scanPath = path;
    filesScanned = 0;
    dirsScanned = 0;
    scanErrors = 0;
    storageView = "scanning";
  }

  function onScanDirectory() {
    storageView = "path-input";
  }

  function onPathConfirm(path: string) {
    scanPath = path;
    filesScanned = 0;
    dirsScanned = 0;
    scanErrors = 0;
    storageView = "scanning";
  }

  function onScanComplete() {
    storageView = "browser";
  }

  function onScanProgress(files: number, dirs: number, errors: number) {
    filesScanned = files;
    dirsScanned = dirs;
    scanErrors = errors;
  }

  function onBackToDrivePicker() {
    storageView = "drive-picker";
  }

  function onDeleteRequest(selections: Map<string, MarkedEntry[]>) {
    deleteSelections = selections;
    storageView = "confirm-delete";
  }

  function onDeleteCancel() {
    storageView = "browser";
  }

  interface DeleteResult {
    path: string;
    success: boolean;
    error: string | null;
    bytes_freed: number;
  }

  function buildAbsolutePath(dirPath: string, name: string): string {
    if (dirPath.endsWith("\\") || dirPath.endsWith("/")) {
      return dirPath + name;
    }
    const sep = dirPath.includes("/") ? "/" : "\\";
    return dirPath + sep + name;
  }

  async function onDeleteConfirm(selections: Map<string, MarkedEntry[]>) {
    const paths: string[] = [];
    for (const [dirPath, entries] of selections) {
      for (const entry of entries) {
        paths.push(buildAbsolutePath(dirPath, entry.name));
      }
    }

    try {
      const results = await invoke<DeleteResult[]>("delete_entries_by_path", {
        paths,
        sortBySize: true,
      });

      const errors = results.filter((r) => !r.success);
      if (errors.length > 0) {
        console.error("Delete errors:", errors);
      }
    } catch (e) {
      console.error("Delete failed:", e);
    }

    storageView = "browser";
    await tick();
    if (directoryViewRef) {
      directoryViewRef.postDelete();
    }
  }

  const ASCII_LOGO = `██████╗ ██╗███████╗██╗  ██╗██╗   ██╗
██╔══██╗██║██╔════╝██║ ██╔╝██║   ██║
██║  ██║██║███████╗█████╔╝ ██║   ██║
██║  ██║██║╚════██║██╔═██╗ ██║   ██║
██████╔╝██║███████║██║  ██╗╚██████╔╝
╚═════╝ ╚═╝╚══════╝╚═╝  ╚═╝ ╚═════╝`;
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="header" onmousedown={startDrag}>
  <pre class="header-logo">{ASCII_LOGO}</pre>
  <div class="header-spacer"></div>
  <TabBar {activeTab} onTabChange={(tab) => activeTab = tab} />
  {#if isWindows}
    <WindowControls />
  {/if}
</div>

<main>
  <!-- Storage tab: stays mounted to preserve scan state -->
  <div style:display={activeTab === "storage" ? "contents" : "none"}>
    {#if storageView === "drive-picker"}
      <DrivePicker onSelect={onDriveSelect} {onScanDirectory} />
    {:else if storageView === "path-input"}
      <PathInput onConfirm={onPathConfirm} onCancel={onBackToDrivePicker} />
    {:else if storageView === "scanning"}
      <Scanning
        path={scanPath}
        {filesScanned}
        {dirsScanned}
        onProgress={onScanProgress}
        onComplete={onScanComplete}
      />
    {/if}

    {#if storageView === "browser" || storageView === "confirm-delete"}
      <div style:display={storageView === "browser" ? "contents" : "none"}>
        <DirectoryView bind:this={directoryViewRef} onQuit={onBackToDrivePicker} onDelete={onDeleteRequest} />
      </div>
    {/if}

    {#if storageView === "confirm-delete"}
      <ConfirmDelete
        selections={deleteSelections}
        onCancel={onDeleteCancel}
        onConfirm={onDeleteConfirm}
      />
    {/if}
  </div>

  <!-- Other tabs: placeholder -->
  {#if activeTab !== "storage"}
    <PlaceholderTab name={tabLabels[activeTab]} />
  {/if}
</main>

<style>
  .header {
    width: 100%;
    flex-shrink: 0;
    -webkit-app-region: drag;
    display: flex;
    align-items: stretch;
    padding: 10px 16px 0 16px;
    border-bottom: 2px solid var(--color-border);
  }

  .header-logo {
    font-size: 5px;
    line-height: 1.15;
    color: var(--color-cyan);
    opacity: 0.7;
    align-self: center;
    -webkit-app-region: no-drag;
  }

  .header-spacer {
    flex: 1;
  }

  main {
    flex: 1;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
  }
</style>
