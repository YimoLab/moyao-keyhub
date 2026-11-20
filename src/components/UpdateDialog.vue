<script setup>
import AppModal from './AppModal.vue'
import AlertBox from '../design/vue/ui/AlertBox.vue'
import Icon from '../design/vue/ui/Icon.vue'
import { downloadAndInstall, dismissUpdate, updateState } from '../lib/updater'
</script>

<template>
  <AppModal
    v-model="updateState.open"
    title="发现新版本"
    :dismissable="!updateState.downloading"
    :closable="!updateState.downloading"
    @close="dismissUpdate"
  >
    <div v-if="updateState.update" class="kh-update">
      <div class="kh-update__versions">
        <span class="anx-badge anx-badge--neutral">当前 v{{ updateState.update.currentVersion }}</span>
        <Icon name="arrow-right" :size="14" />
        <span class="anx-badge anx-badge--ok">v{{ updateState.update.version }}</span>
        <span v-if="updateState.update.date" class="anx-t-xs anx-t-muted">
          {{ updateState.update.date.slice(0, 10) }}
        </span>
      </div>

      <div v-if="updateState.update.body" class="od-field anx-field">
        <span class="anx-field__label">更新说明</span>
        <pre class="kh-pre kh-update__notes">{{ updateState.update.body }}</pre>
      </div>

      <div v-if="updateState.downloading" class="kh-update__progress">
        <div class="kh-meter__track">
          <span
            class="kh-meter__fill kh-meter__fill--ok"
            :style="{ width: updateState.progress + '%' }"
          />
        </div>
        <div class="kh-update__progress-label">
          <span>{{ updateState.installing ? '正在安装，应用即将重启…' : '正在下载更新包…' }}</span>
          <span>{{ updateState.progress }}%</span>
        </div>
      </div>

      <AlertBox
        v-if="updateState.error"
        tone="danger"
        title="更新失败"
        :text="updateState.error"
      />

      <p class="kh-fine">
        更新包会先用内置公钥校验签名，校验不通过不会安装。安装完成后应用会自动重启。
      </p>
    </div>

    <template #footer>
      <button
        class="anx-btn anx-btn--ghost"
        :disabled="updateState.downloading"
        @click="dismissUpdate"
      >
        稍后
      </button>
      <button
        class="anx-btn anx-btn--primary"
        :disabled="updateState.downloading"
        @click="downloadAndInstall"
      >
        <span v-if="updateState.downloading" class="anx-btn__spinner" />
        {{ updateState.downloading ? '更新中…' : '下载并安装' }}
      </button>
    </template>
  </AppModal>
</template>
