<script setup lang="ts">
import { computed } from "vue";
import { formatDate } from "../lib/api";
import type { Category, Transaction } from "../lib/api";

const props = defineProps<{ tx: Transaction; categories: Category[] }>();
const emit = defineEmits<{ goto: [id: number] }>();

function label(key: string): string {
  return props.categories.find((c) => c.key === key)?.label ?? key;
}

const changes = computed(() => {
  let from = props.tx.original_category;
  return props.tx.classification_history.map((h) => {
    const line = `Category changed from ${label(from)} to ${label(h.category)}: ${h.reason}`;
    from = h.category;
    return line;
  });
});
</script>

<template>
  <div class="flex flex-col gap-1 border-l border-rule pl-4 text-caption text-muted">
    <p v-if="tx.void">
      Voided {{ formatDate(tx.void.recorded_at) }}: {{ tx.void.reason }}
    </p>
    <p v-if="tx.replaced_by_id !== null">
      <button
        type="button"
        class="cursor-pointer underline decoration-accent underline-offset-4 hover:text-fg"
        @click="emit('goto', tx.replaced_by_id)"
      >Corrected by entry #{{ tx.replaced_by_id }}</button>
    </p>
    <p v-if="tx.replaces_id !== null">
      <button
        type="button"
        class="cursor-pointer underline decoration-accent underline-offset-4 hover:text-fg"
        @click="emit('goto', tx.replaces_id)"
      >Corrects entry #{{ tx.replaces_id }}</button>
    </p>
    <p v-for="(line, i) in changes" :key="i">{{ line }}</p>
  </div>
</template>
