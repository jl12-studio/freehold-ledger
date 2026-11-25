<script setup lang="ts">
import { computed } from "vue";
import type { Business } from "../lib/api";

const props = defineProps<{ businesses: Business[]; modelValue: number; currentId: number | null }>();
const emit = defineEmits<{ "update:modelValue": [id: number] }>();

const active = computed(() => props.businesses.filter((b) => !b.archived));
const archived = computed(() => props.businesses.filter((b) => b.archived));
const selected = computed(() => props.businesses.find((b) => b.id === props.modelValue) ?? null);
const current = computed(() => props.businesses.find((b) => b.id === props.currentId) ?? null);
const recordsHere = computed(() => props.currentId !== null && props.currentId === props.modelValue);

const caption = computed(() => {
  if (recordsHere.value) return "Your agent records here";
  if (current.value) return `Your agent records in ${current.value.name}`;
  return "No business is current for your agent";
});

const optionText = (b: Business) => (b.is_test ? `${b.name} (test)` : b.name);

function onChange(e: Event) {
  emit("update:modelValue", Number((e.target as HTMLSelectElement).value));
}
</script>

<template>
  <div class="flex min-w-0 items-baseline gap-2">
    <span class="hidden font-serif text-[1.375rem] leading-none text-rule lg:inline" aria-hidden="true">/</span>
    <label class="sr-only" for="business-select">Business</label>
    <select
      v-if="businesses.length > 1"
      id="business-select"
      class="book-select max-w-[16rem]"
      :value="modelValue"
      @change="onChange"
    >
      <option v-for="b in active" :key="b.id" :value="b.id" :selected="b.id === modelValue">{{ optionText(b) }}</option>
      <optgroup v-if="archived.length > 0" label="Archived">
        <option v-for="b in archived" :key="b.id" :value="b.id" :selected="b.id === modelValue">{{ optionText(b) }}</option>
      </optgroup>
    </select>
    <span v-else class="font-serif text-[1.1875rem] font-[450] italic">{{ selected?.name }}</span>
    <span v-if="selected?.is_test" class="chip chip-test self-center">Test</span>
    <span v-if="selected?.archived" class="chip chip-muted self-center">Archived</span>
    <span
      :class="['label hidden truncate md:inline', recordsHere ? 'agent-here' : '']"
      :title="caption"
    >{{ caption }}</span>
  </div>
</template>
