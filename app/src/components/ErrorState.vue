<script setup lang="ts">
import type { AppError } from "../lib/api";

defineProps<{ error: AppError }>();
defineEmits<{ reveal: [] }>();
</script>

<template>
  <section class="max-w-measure mx-auto px-6 pt-16">
    <h2 class="font-serif text-h2 font-semibold">This ledger could not be opened</h2>
    <p class="mt-4 font-serif text-body">{{ error.message }}</p>
    <p class="mt-3">
      <span class="font-mono text-caption break-all text-muted">{{ error.path }}</span>
    </p>
    <p v-if="!error.message.includes('Nothing was changed.')" class="mt-3 font-serif text-body">Nothing was changed.</p>
    <button
      type="button"
      class="btn mt-6"
      @click="$emit('reveal')"
    >
      Show in Finder
    </button>
  </section>
</template>
