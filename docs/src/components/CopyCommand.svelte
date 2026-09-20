<script>
  let { command = '' } = $props();
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }
</script>

<div
  class="not-content flex items-center gap-3 rounded-lg border border-gray-300 bg-gray-50 px-4 py-3 font-mono text-sm dark:border-gray-700 dark:bg-gray-900"
>
  <code class="overflow-x-auto whitespace-pre">{command}</code>
  <button
    type="button"
    class="ml-auto shrink-0 cursor-pointer rounded-md border border-gray-300 px-2 py-1 text-xs transition hover:bg-gray-200 dark:border-gray-600 dark:hover:bg-gray-800"
    onclick={copy}
  >
    {copied ? 'copied' : 'copy'}
  </button>
</div>
