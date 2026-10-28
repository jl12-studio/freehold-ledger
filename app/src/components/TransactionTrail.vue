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

type Item =
  | { kind: "text"; key: string; text: string }
  | { kind: "goto"; key: string; text: string; id: number };

const items = computed<Item[]>(() => {
  const out: Item[] = [];
  if (props.tx.void) {
    out.push({ kind: "text", key: "void", text: `Voided ${formatDate(props.tx.void.recorded_at)}: ${props.tx.void.reason}` });
  }
  if (props.tx.replaced_by_id !== null) {
    out.push({ kind: "goto", key: "by", text: `Corrected by #${props.tx.replaced_by_id}`, id: props.tx.replaced_by_id });
  }
  if (props.tx.replaces_id !== null) {
    out.push({ kind: "goto", key: "corrects", text: `Corrects #${props.tx.replaces_id}`, id: props.tx.replaces_id });
  }
  changes.value.forEach((text, i) => out.push({ kind: "text", key: `c${i}`, text }));
  return out;
});
</script>

<template>
  <p class="flex flex-wrap gap-x-3 gap-y-0.5 text-caption text-muted">
    <template v-for="(item, i) in items" :key="item.key">
      <span v-if="i > 0" aria-hidden="true">·</span>
      <button v-if="item.kind === 'goto'" type="button" class="link text-caption" @click="emit('goto', item.id)">{{ item.text }}</button>
      <span v-else>{{ item.text }}</span>
    </template>
  </p>
</template>
