<script setup lang="ts">
import type { AppError } from "../lib/api";

defineProps<{ error: AppError }>();
defineEmits<{ reveal: [] }>();
</script>

<template>
  <section class="max-w-measure mx-auto px-6 pt-16">
    <h2 class="font-serif text-h2 font-semibold">This ledger could not be opened</h2>
    <p class="mt-4 font-serif text-body">{{ error.message }}</p>
    <p class="mt-3 text-ui text-muted">
      <span class="font-mono break-all">{{ error.path }}</span>
    </p>
    <p class="mt-3 font-serif text-body">Nothing was changed.</p>
    <button
      type="button"
      class="mt-6 min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
      @click="$emit('reveal')"
    >
      Show in Finder
    </button>
  </section>
</template>
