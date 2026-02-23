<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const appWindow = getCurrentWindow();

  let maximized = $state(false);

  async function updateMaximized() {
    maximized = await appWindow.isMaximized();
  }

  updateMaximized();

  appWindow.onResized(() => {
    updateMaximized();
  });
</script>

<div class="window-controls">
  <button class="control-btn" onclick={() => appWindow.minimize()} aria-label="Minimize">
    <svg width="10" height="1" viewBox="0 0 10 1">
      <rect width="10" height="1" fill="currentColor" />
    </svg>
  </button>

  <button class="control-btn" onclick={() => appWindow.toggleMaximize()} aria-label={maximized ? "Restore" : "Maximize"}>
    {#if maximized}
      <svg width="10" height="10" viewBox="0 0 10 10">
        <path d="M2 0h6v2h2v6H8v2H0V4h2V0zm1 1v2h5v5h1V2H3zm-2 4v4h6V5H1z" fill="currentColor" fill-rule="evenodd" />
      </svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10">
        <rect x="0" y="0" width="10" height="10" stroke="currentColor" stroke-width="1" fill="none" />
      </svg>
    {/if}
  </button>

  <button class="control-btn close-btn" onclick={() => appWindow.close()} aria-label="Close">
    <svg width="10" height="10" viewBox="0 0 10 10">
      <path d="M1 0L5 4L9 0L10 1L6 5L10 9L9 10L5 6L1 10L0 9L4 5L0 1Z" fill="currentColor" />
    </svg>
  </button>
</div>

<style>
  .window-controls {
    display: flex;
    -webkit-app-region: no-drag;
  }

  .control-btn {
    width: 46px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0;
    transition: background 0.1s;
  }

  .control-btn:hover {
    background: var(--bg-hover);
  }

  .close-btn:hover {
    background: #c42b1c;
    color: #fff;
  }
</style>
