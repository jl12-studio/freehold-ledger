<script setup lang="ts">
defineProps<{ path: string | null }>();
defineEmits<{ reveal: [] }>();
</script>

<template>
  <header class="flex items-center justify-between gap-6 border-b border-rule px-6 py-4">
    <h1 class="font-serif text-h3 font-semibold">Freehold Ledger</h1>
    <div v-if="path" class="flex min-w-0 items-center gap-4">
      <span class="truncate max-w-[28rem] font-mono text-caption text-muted" :title="path">{{
        path
      }}</span>
      <button
        type="button"
        class="min-h-8 shrink-0 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('reveal')"
      >
        Show in Finder
      </button>
    </div>
  </header>
</template>
