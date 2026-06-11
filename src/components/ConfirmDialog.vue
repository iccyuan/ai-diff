<script setup lang="ts">
import { confirmState, settleConfirm } from "../lib/confirm";
</script>

<template>
  <Teleport to="body">
    <div v-if="confirmState.open" class="modal-mask" @click.self="settleConfirm(false)">
      <div class="modal">
        <h3>{{ confirmState.title }}</h3>
        <p>{{ confirmState.message }}</p>
        <div class="modal-actions">
          <button class="btn" @click="settleConfirm(false)">取消</button>
          <button class="btn danger" @click="settleConfirm(true)">确定</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
