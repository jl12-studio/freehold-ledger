<script setup lang="ts">
defineEmits<{ reveal: [] }>();
</script>

<template>
  <section class="max-w-measure mx-auto px-6 pt-16">
    <h2 class="font-serif text-h2 font-semibold">Your ledger is ready</h2>
    <p class="mt-4 font-serif text-body">
      Nothing has been recorded yet. Your agent adds entries by conversation once it is connected.
    </p>
    <p class="mt-3 text-ui text-muted">
      This file is yours: copy it, back it up, open it with any SQLite tool.
    </p>
    <button
      type="button"
      class="mt-6 min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
      @click="$emit('reveal')"
    >
      Show the ledger file
    </button>
  </section>
</template>
