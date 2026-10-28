<script setup lang="ts">
import ExportControl from "./ExportControl.vue";

defineProps<{ path: string | null; showExport?: boolean }>();
defineEmits<{ reveal: [] }>();
</script>

<template>
  <header class="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b border-rule px-6 py-4">
    <h1 class="font-serif text-h3 font-semibold">Freehold Ledger</h1>
    <div class="flex min-w-0 flex-wrap items-center gap-x-6 gap-y-2">
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
      <!-- "Connect your agent" (M3) is not in the header yet; Export sits at the end of the action area. -->
      <ExportControl v-if="showExport" />
    </div>
  </header>
</template>
