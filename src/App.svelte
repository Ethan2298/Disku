<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import StartScreen from "./lib/StartScreen.svelte";
  import DrivePicker from "./lib/DrivePicker.svelte";
  import PathInput from "./lib/PathInput.svelte";
  import Scanning from "./lib/Scanning.svelte";
  import Browser from "./lib/Browser.svelte";

  function startDrag(e: MouseEvent) {
    if (e.buttons === 1) {
      getCurrentWindow().startDragging();
    }
  }

  type View =
    | "start"
    | "drive-picker"
    | "path-input"
    | "scanning"
    | "browser";

  let currentView: View = $state("start");
  let scanPath: string = $state("");
  let filesScanned: number = $state(0);
  let scanErrors: number = $state(0);

  function onMenuSelect(choice: string) {
    if (choice === "volume") {
      currentView = "drive-picker";
    } else if (choice === "directory") {
      currentView = "path-input";
    } else if (choice === "quit") {
      // In Tauri, we don't actually quit from the browser
      // but you could call: import { exit } from '@tauri-apps/plugin-process';
    }
  }

  function onDriveSelect(path: string) {
    scanPath = path;
    filesScanned = 0;
    scanErrors = 0;
    currentView = "scanning";
  }

  function onPathConfirm(path: string) {
    scanPath = path;
    filesScanned = 0;
    scanErrors = 0;
    currentView = "scanning";
  }

  function onScanComplete() {
    currentView = "browser";
  }

  function onScanProgress(files: number, errors: number) {
    filesScanned = files;
    scanErrors = errors;
  }

  function onBackToStart() {
    currentView = "start";
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="titlebar" onmousedown={startDrag}></div>

<main>
  {#if currentView === "start"}
    <StartScreen onSelect={onMenuSelect} />
  {:else if currentView === "drive-picker"}
    <DrivePicker onSelect={onDriveSelect} onBack={onBackToStart} />
  {:else if currentView === "path-input"}
    <PathInput onConfirm={onPathConfirm} onCancel={onBackToStart} />
  {:else if currentView === "scanning"}
    <Scanning
      path={scanPath}
      {filesScanned}
      onProgress={onScanProgress}
      onComplete={onScanComplete}
    />
  {:else if currentView === "browser"}
    <Browser onQuit={onBackToStart} />
  {/if}
</main>

<style>
  .titlebar {
    height: 36px;
    width: 100%;
    flex-shrink: 0;
    -webkit-app-region: drag;
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
