<script setup lang="ts">
import { computed } from "vue";
import BusinessSwitcher from "./BusinessSwitcher.vue";
import ExportControl from "./ExportControl.vue";
import type { Business } from "../lib/api";

const props = defineProps<{
  path: string | null;
  showExport?: boolean;
  businesses: Business[];
  selectedId: number | null;
  currentId: number | null;
}>();
defineEmits<{ reveal: []; connect: []; select: [id: number] }>();

const selected = computed(() => props.businesses.find((b) => b.id === props.selectedId) ?? null);
</script>

<template>
  <header class="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 border-b border-rule px-6 py-4">
    <h1 class="font-serif text-h3 font-semibold">Freehold Ledger</h1>
    <div class="flex min-w-0 flex-wrap items-center gap-x-6 gap-y-2">
      <BusinessSwitcher
        v-if="selected"
        :businesses="businesses"
        :model-value="selected.id"
        :current-id="currentId"
        @update:model-value="$emit('select', $event)"
      />
      <button
        type="button"
        class="min-h-8 shrink-0 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('connect')"
      >
        Connect your agent
      </button>
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
      <ExportControl v-if="showExport && selected" :business-id="selected.id" :business-name="selected.name" />
    </div>
  </header>
</template>
