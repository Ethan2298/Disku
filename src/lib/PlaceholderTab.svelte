<script lang="ts">
  interface Props {
    name: string;
  }

  let { name }: Props = $props();
</script>

<div class="placeholder">
  <span class="placeholder-name">{name}</span>
  <span class="placeholder-label">Coming Soon</span>
</div>

<style>
  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    gap: 8px;
  }

  .placeholder-name {
    color: var(--text-muted);
    font-size: 20px;
    font-weight: 500;
  }

  .placeholder-label {
    color: var(--text-muted);
    font-size: 13px;
  }
</style>
