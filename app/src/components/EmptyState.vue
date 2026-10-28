<script setup lang="ts">
defineEmits<{ reveal: []; connect: [] }>();
</script>

<template>
  <section class="max-w-measure mx-auto px-6 pt-16">
    <h2 class="font-serif text-h2 font-semibold">Your ledger is ready</h2>
    <p class="mt-4 font-serif text-body">
      Nothing has been recorded yet. Connect your agent and tell it what you bought or earned.
    </p>
    <p class="mt-3 text-ui text-muted">
      This file is yours: copy it, back it up, open it with any SQLite tool.
    </p>
    <div class="mt-6 flex flex-wrap items-center gap-x-6 gap-y-2">
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('connect')"
      >
        Connect your agent
      </button>
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('reveal')"
      >
        Show the ledger file
      </button>
    </div>
  </section>
</template>
