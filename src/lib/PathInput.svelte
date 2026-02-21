<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface Props {
    onConfirm: (path: string) => void;
    onCancel: () => void;
  }

  let { onConfirm, onCancel }: Props = $props();

  let inputValue: string = $state("");
  let errorMessage: string = $state("");
  let inputEl: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (inputEl) inputEl.focus();
  });

  async function handleSubmit() {
    if (!inputValue.trim()) return;

    try {
      const valid = await invoke<boolean>("validate_path", {
        path: inputValue,
      });
      if (valid) {
        onConfirm(inputValue);
      } else {
        errorMessage = "not a valid directory";
      }
    } catch {
      errorMessage = "failed to validate path";
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    } else if (e.key === "Enter") {
      e.preventDefault();
      handleSubmit();
    }
    // Clear error on typing
    if (errorMessage && e.key.length === 1) {
      errorMessage = "";
    }
  }
</script>

<div class="path-input">
  <div class="panel">
    <div class="panel-title">scan directory</div>
    <div class="panel-content">
      <label class="label" for="path-input">path:</label>
      <div class="input-row">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="path-input"
          bind:this={inputEl}
          bind:value={inputValue}
          onkeydown={handleKeydown}
          class="text-input"
          type="text"
          placeholder="/path/to/directory"
          autofocus
        />
      </div>
      {#if errorMessage}
        <p class="error">{errorMessage}</p>
      {/if}
    </div>
    <div class="panel-footer">enter confirm · esc cancel</div>
  </div>
</div>

<style>
  .path-input {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
  }

  .panel {
    border: 1px solid var(--color-border);
    width: 50%;
    min-width: 350px;
    max-width: 600px;
    display: flex;
    flex-direction: column;
  }

  .panel-title {
    padding: 4px 8px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }

  .panel-content {
    padding: 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .panel-footer {
    padding: 4px 8px;
    color: var(--text-muted);
    border-top: 1px solid var(--color-border);
    font-size: 12px;
  }

  .label {
    color: var(--text-secondary);
    font-size: 13px;
  }

  .input-row {
    display: flex;
  }

  .text-input {
    flex: 1;
    background: var(--bg-secondary);
    border: 1px solid var(--color-border);
    color: white;
    font-family: inherit;
    font-size: 13px;
    padding: 6px 8px;
    outline: none;
    caret-color: var(--color-accent);
  }

  .text-input:focus {
    border-color: var(--color-accent-dim);
  }

  .error {
    color: var(--color-red);
    font-size: 12px;
  }
</style>
