<script setup lang="ts">
import { computed } from "vue";
import BrandMark from "./BrandMark.vue";
import BusinessSwitcher from "./BusinessSwitcher.vue";
import ExportControl from "./ExportControl.vue";
import type { Business } from "../lib/api";

const props = defineProps<{
  showExport?: boolean;
  businesses: Business[];
  selectedId: number | null;
  currentId: number | null;
}>();
defineEmits<{ connect: []; select: [id: number]; year: [year: number] }>();

const selected = computed(() => props.businesses.find((b) => b.id === props.selectedId) ?? null);
</script>

<template>
  <header class="sticky top-0 z-20 border-b border-rule bg-bg/90 backdrop-blur-md">
    <div
      class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-6 gap-y-3 px-6 py-3 lg:gap-x-4 xl:gap-x-6 lg:grid-cols-[auto_minmax(0,1fr)_auto_auto]"
    >
      <div class="col-start-1 row-start-1 flex items-center gap-2.5">
        <BrandMark class="size-[1.375rem] shrink-0" />
        <h1 class="font-serif text-[1.375rem] leading-none font-[620] tracking-[-0.012em]">Freehold Ledger</h1>
      </div>
      <BusinessSwitcher
        v-if="selected"
        class="col-start-1 row-start-2 lg:col-start-2 lg:row-start-1"
        :businesses="businesses"
        :model-value="selected.id"
        :current-id="currentId"
        @update:model-value="$emit('select', $event)"
      />
      <ExportControl
        v-if="showExport && selected"
        :business-id="selected.id"
        :business-name="selected.name"
        @year="$emit('year', $event)"
      />
      <button
        type="button"
        class="btn btn-primary col-start-2 row-start-1 lg:col-start-4"
        @click="$emit('connect')"
      >
        Connect your agent
      </button>
    </div>
  </header>
</template>
