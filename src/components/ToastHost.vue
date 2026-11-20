<script setup>
import { useToastState } from '../lib/toast.js'

const { items } = useToastState()

const CHECK = 'M5 13l4 4 10-11'
const BANG = 'M12 5v9M12 17.5v.5'
</script>

<template>
  <div class="toasts" aria-live="polite">
    <TransitionGroup name="toast-list">
      <div v-for="t in items" :key="t.id" class="toast" :class="`t-${t.variant}`">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4">
          <path :d="t.variant === 'danger' ? BANG : CHECK" />
        </svg>
        <span>{{ t.message }}</span>
      </div>
    </TransitionGroup>
  </div>
</template>
