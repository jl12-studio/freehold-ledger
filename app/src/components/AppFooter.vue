<script setup lang="ts">
defineProps<{ path: string }>();
defineEmits<{ reveal: [] }>();
</script>

<template>
  <footer class="mt-auto border-t border-rule">
    <div class="flex items-center gap-3 px-6 py-3">
      <span class="label shrink-0">Ledger file</span>
      <span class="min-w-0 truncate font-mono text-caption text-muted" :title="path">{{ path }}</span>
      <button type="button" class="link shrink-0 text-caption" @click="$emit('reveal')">Show in Finder</button>
      <span class="ml-auto hidden shrink-0 font-serif text-caption italic text-muted md:inline">Yours to copy, back up, or open anywhere.</span>
    </div>
  </footer>
</template>
