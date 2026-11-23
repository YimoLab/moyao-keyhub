<script setup>
/**
 * 页面骨架屏
 *
 * 数据加载期间用与真实布局同构的占位块替代空白/转圈，避免内容突然撑开造成跳动。
 * 变体与页面一一对应；基础微光样式复用设计系统的 .anx-sk（components.css §30）。
 *
 * 用法：<PageSkeleton v-if="loading" variant="vault" />
 */
defineProps({
  /** vault | config | security | settings | env | diagnostics | generic */
  variant: { type: String, default: 'generic' },
})
</script>

<template>
  <!-- 密钥库：统计卡 + 工具条 + 列表 -->
  <div v-if="variant === 'vault'" class="kh-sk-stack" role="status" aria-label="正在加载密钥库">
    <div class="anx-grid-4">
      <span v-for="i in 4" :key="i" class="anx-sk kh-sk-stat" />
    </div>
    <div class="kh-sk-inline">
      <span class="anx-sk kh-sk-field kh-sk-field--search" />
      <span class="anx-sk kh-sk-field kh-sk-field--sm" />
      <span class="anx-sk kh-sk-field kh-sk-field--sm" />
    </div>
    <div class="anx-panel">
      <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
      <div class="kh-sk-rows">
        <span v-for="i in 6" :key="i" class="anx-sk kh-sk-row" />
      </div>
    </div>
  </div>

  <!-- SSH Config：Host 列表 + 表单 + 校验栏 -->
  <div v-else-if="variant === 'config'" class="kh-sk-cfg" role="status" aria-label="正在加载 SSH Config">
    <div class="anx-panel kh-sk-panelbox">
      <div class="anx-panel__head"><span class="anx-sk kh-sk-field" /></div>
      <div class="kh-sk-rows">
        <span v-for="i in 5" :key="i" class="anx-sk kh-sk-row kh-sk-row--short" />
      </div>
    </div>
    <div class="anx-panel kh-sk-panelbox">
      <div class="anx-panel__head">
        <span class="anx-sk kh-sk-title" />
        <span class="anx-sk kh-sk-badge" />
      </div>
      <div class="kh-sk-fieldgrid">
        <span v-for="i in 4" :key="i" class="anx-sk kh-sk-field" />
      </div>
      <div class="kh-sk-rows">
        <span v-for="i in 3" :key="i" class="anx-sk kh-sk-row" />
      </div>
    </div>
    <div class="anx-panel kh-sk-panelbox kh-sk-panelbox--side">
      <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
      <div class="kh-sk-rows">
        <span v-for="i in 4" :key="i" class="anx-sk kh-sk-row" />
      </div>
    </div>
  </div>

  <!-- 安全中心：评分 + 体检 + 审计 -->
  <div v-else-if="variant === 'security'" class="kh-sk-stack" role="status" aria-label="正在加载安全中心">
    <div class="kh-sk-sec-hero">
      <div class="anx-panel kh-sk-panelbox">
        <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
        <div class="kh-sk-score">
          <span class="anx-sk kh-sk-score__num" />
          <span class="anx-sk kh-sk-row kh-sk-row--short" />
          <span class="anx-sk kh-sk-row kh-sk-row--short" />
          <span class="anx-sk kh-sk-row kh-sk-row--short" />
        </div>
      </div>
      <div class="anx-panel kh-sk-panelbox">
        <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
        <div class="kh-sk-rows">
          <span v-for="i in 5" :key="i" class="anx-sk kh-sk-row" />
        </div>
      </div>
    </div>
    <div class="anx-panel kh-sk-panelbox">
      <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
      <div class="kh-sk-rows">
        <span v-for="i in 6" :key="i" class="anx-sk kh-sk-row" />
      </div>
    </div>
  </div>

  <!-- 设置：两栏面板 -->
  <div v-else-if="variant === 'settings'" class="kh-sk-settings" role="status" aria-label="正在加载设置">
    <div v-for="col in 2" :key="col" class="kh-sk-settings__col">
      <div v-for="i in 3" :key="i" class="anx-panel kh-sk-panelbox">
        <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
        <div class="kh-sk-rows">
          <span v-for="j in 3" :key="j" class="anx-sk kh-sk-row" />
        </div>
      </div>
    </div>
  </div>

  <!-- 环境自检：四张检测卡 -->
  <div v-else-if="variant === 'env'" class="kh-sk-stack" role="status" aria-label="正在检测环境">
    <div class="kh-env-grid">
      <div v-for="i in 4" :key="i" class="anx-panel kh-sk-envcard">
        <div class="kh-sk-envcard__head">
          <span class="anx-sk kh-sk-icon" />
          <div class="kh-sk-grow">
            <span class="anx-sk kh-sk-title" />
            <span class="anx-sk kh-sk-line" />
          </div>
          <span class="anx-sk kh-sk-badge" />
        </div>
        <div class="kh-sk-envcard__body">
          <span class="anx-sk kh-sk-line" />
        </div>
      </div>
    </div>
  </div>

  <!-- 连接测试：表单 + 终端 + 结果 -->
  <div v-else-if="variant === 'diagnostics'" class="kh-sk-stack" role="status" aria-label="正在加载连接测试">
    <div class="kh-sk-split">
      <div class="anx-panel kh-sk-panelbox">
        <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
        <div class="kh-sk-fieldgrid">
          <span v-for="i in 4" :key="i" class="anx-sk kh-sk-field" />
        </div>
        <div class="kh-sk-rows">
          <span class="anx-sk kh-sk-row" />
          <span class="anx-sk kh-sk-term" />
        </div>
      </div>
      <div class="anx-panel kh-sk-panelbox">
        <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
        <div class="kh-sk-rows">
          <span v-for="i in 4" :key="i" class="anx-sk kh-sk-row" />
        </div>
      </div>
    </div>
  </div>

  <!-- 通用：标题 + 三卡 + 行列表 -->
  <div v-else class="kh-sk-stack" role="status" aria-label="页面加载中">
    <div class="kh-sk-heading">
      <span class="anx-sk kh-sk-title" />
      <span class="anx-sk kh-sk-line" />
    </div>
    <div class="kh-sk-cards">
      <span v-for="i in 3" :key="i" class="anx-sk kh-sk-card" />
    </div>
    <div class="anx-panel">
      <div class="anx-panel__head"><span class="anx-sk kh-sk-title" /></div>
      <div class="kh-sk-rows">
        <span v-for="i in 6" :key="i" class="anx-sk kh-sk-row" />
      </div>
    </div>
  </div>
</template>
