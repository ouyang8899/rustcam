<template>
  <div class="page-root">
    <div class="body-content">
      <el-row :gutter="12" align="middle" class="page-toolbar">
        <el-col :span="12">
          <el-button :icon="ArrowLeft" @click="$router.push('/devices')">返回</el-button>
          <el-text style="margin-left: 12px">{{ device?.name || '设备详情' }}</el-text>
          <el-tag v-if="device?.protocol" style="margin-left: 8px" size="small">{{ device.protocol }}</el-tag>
          <el-tag v-if="isRtspPush" style="margin-left: 4px" size="small" type="success">推送</el-tag>
          <el-tag v-if="isRtspPull" style="margin-left: 4px" size="small" type="warning">拉取</el-tag>
        </el-col>
        <el-col :span="12" style="text-align: right">
          <el-button :icon="Refresh" @click="fetchPlayLinks">刷新链接</el-button>
          <el-button type="primary" :icon="Edit" @click="$router.push({ path: '/devices', query: { edit: route.params.id } })">编辑设备</el-button>
          <el-button type="warning" :icon="Close" @click="toggleMaintenanceAndStop">维护</el-button>
        </el-col>
      </el-row>

      <el-skeleton animated :loading="loading" :rows="6">
        <template #default>
          <div v-if="device">
            <el-row :gutter="12">
              <el-col :span="16">
                <el-card>
                  <template #header><span>实时播放</span></template>
                  <el-segmented v-model="activePlayerTab" :options="playerTabs" style="margin-bottom: 12px" />
                  <div v-if="activePlayerTab === 'player'">
                    <VideoPlayer
                      v-if="playLinks"
                      :liveSrc="activeStreamUrl"
                      :type="activeStreamType"
                      :isLive="true"
                      :autoplay="true"
                    />
                    <el-empty v-else description="点击「获取播放链接」以开始播放" />
                  </div>
                  <div v-else>
                    <el-descriptions :column="2" border size="small">
                      <el-descriptions-item v-for="link in availablePlayLinks" :key="link.key" :label="link.label">
                        <div style="display: flex; gap: 6px; align-items: center">
                          <el-tag size="small" :type="link.tagType">{{ link.tag }}</el-tag>
                          <el-text v-if="link.url" truncated style="flex: 1; font-family: monospace; font-size: 11px">{{ link.url }}</el-text>
                          <el-text v-else type="info">不支持</el-text>
                          <el-button v-if="link.url" size="small" @click="playStream(link)">播放</el-button>
                          <el-button v-if="link.url" size="small" @click="copyLink(link.url)">复制</el-button>
                        </div>
                      </el-descriptions-item>
                    </el-descriptions>
                  </div>
                </el-card>

                <el-card style="margin-top: 12px" v-if="isRtspPull || isRtspPush || isOnvif || isGb28181">
                  <template #header><span>流媒体配置</span></template>
                  <el-descriptions :column="2" border size="small">
                    <template v-if="isRtspPull">
                      <el-descriptions-item label="RTSP 全链接">
                        <el-text truncated style="font-family: monospace; font-size: 12px">{{ device.extended?.rtsp_full_url || device.rtsp_full_url || '-' }}</el-text>
                      </el-descriptions-item>
                      <el-descriptions-item label="认证用户">{{ device.device_username || '-' }}</el-descriptions-item>
                    </template>
                    <template v-if="isRtspPush">
                      <el-descriptions-item label="媒体服务器">{{ device.media_server_name || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="应用名称 (App)">{{ device.app || 'live' }}</el-descriptions-item>
                      <el-descriptions-item label="流标识">{{ device.stream_key || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="推送地址">
                        <el-text v-if="device.media_server_name && device.stream_key" style="font-family: monospace; font-size: 12px" type="success">
                          {{ mediaServerPushUrl }}
                        </el-text>
                        <el-text v-else type="info">-</el-text>
                      </el-descriptions-item>
                    </template>
                    <template v-if="isRtmpPush">
                      <el-descriptions-item label="媒体服务器">{{ device.media_server_name || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="流标识">{{ device.stream_key || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="推送地址">
                        <el-text v-if="device.media_server_name && device.stream_key" style="font-family: monospace; font-size: 12px" type="success">
                          {{ rtmpPushUrl }}
                        </el-text>
                        <el-text v-else type="info">-</el-text>
                      </el-descriptions-item>
                    </template>
                    <template v-if="isOnvif">
                      <el-descriptions-item label="设备地址">{{ device.extended?.x_addr || device.host || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="厂商">{{ device.extended?.manufacturer || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="型号">{{ device.extended?.model || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="能力">
                        <span v-if="device.extended?.capabilities?.media" style="color: var(--el-color-success)">媒体</span>
                        <span v-if="device.extended?.capabilities?.ptz" style="color: var(--el-color-success)"> PTZ</span>
                        <span v-if="device.extended?.capabilities?.events" style="color: var(--el-color-success)"> 事件</span>
                      </el-descriptions-item>
                      <el-descriptions-item v-if="device.pull_urls?.[0]?.url" label="RTSP 拉流地址">
                        <div style="display: flex; align-items: center; gap: 6px">
                          <el-text truncated style="font-family: monospace; font-size: 11px; max-width: 200px">{{ device.pull_urls[0].url }}</el-text>
                          <el-button size="small" @click="openPreview(device.pull_urls[0].url)" title="预览">▶</el-button>
                          <el-button size="small" @click="copyLink(device.pull_urls[0].url)" title="复制"><el-icon><CopyDocument /></el-icon></el-button>
                        </div>
                      </el-descriptions-item>
                    </template>
                    <template v-if="isGb28181">
                      <el-descriptions-item label="SIP ID">{{ device.extended?.gb_id || device.gb_id || device.device_tag || '-' }}</el-descriptions-item>
                      <el-descriptions-item label="所属地区">{{ device.region_code || '-' }}</el-descriptions-item>
                    </template>
                    <template v-if="device.stream_config && Object.keys(device.stream_config).length > 0">
                      <el-descriptions-item label="视频编码">{{ device.stream_config.video_codec || 'PS' }}</el-descriptions-item>
                      <el-descriptions-item label="音频编码">{{ device.stream_config.audio_codec || 'PCMA' }}</el-descriptions-item>
                      <el-descriptions-item label="视频PT">{{ device.stream_config.video_payload_type ?? 96 }}</el-descriptions-item>
                      <el-descriptions-item label="音频PT">{{ device.stream_config.audio_payload_type ?? 8 }}</el-descriptions-item>
                      <el-descriptions-item label="Profile">{{ device.stream_config.profile_level_id || '4D001F' }}</el-descriptions-item>
                      <el-descriptions-item label="流模式">{{ device.stream_config.stream_mode || 'recvonly' }}</el-descriptions-item>
                    </template>
                  </el-descriptions>
                </el-card>

                <el-card style="margin-top: 12px" v-if="device.push_urls?.length || device.pull_urls?.length">
                  <template #header><span>原始流地址</span></template>
                  <div v-if="device.push_urls?.length">
                    <el-text type="info" size="small">推流地址 ({{ device.push_urls.length }})</el-text>
                    <div v-for="(url, i) in device.push_urls" :key="'push-'+i" style="margin-top: 4px">
                      <el-tag size="small">{{ url.protocol }} P{{ url.priority }}</el-tag>
                      <el-text truncated style="font-family: monospace; font-size: 11px; max-width: 400px">{{ url.url }}</el-text>
                    </div>
                  </div>
                  <div v-if="device.pull_urls?.length" style="margin-top: 8px">
                    <el-text type="info" size="small">拉流地址 ({{ device.pull_urls.length }})</el-text>
                    <div v-for="(url, i) in device.pull_urls" :key="'pull-'+i" style="margin-top: 4px">
                      <el-tag size="small">{{ url.protocol }} P{{ url.priority }}</el-tag>
                      <el-text truncated style="font-family: monospace; font-size: 11px; max-width: 400px">{{ url.url }}</el-text>
                    </div>
                  </div>
                </el-card>
              </el-col>

              <el-col :span="8">
                <el-card>
                  <template #header><span>设备信息</span></template>
                  <el-descriptions :column="1" border size="small">
                    <el-descriptions-item label="状态">
                      <span class="status-dot" :class="getStatusClass(device.status)" style="margin-right: 6px" />{{ device.status }}
                    </el-descriptions-item>
                    <el-descriptions-item label="设备类型">
                      <el-tag v-if="device.is_channel" size="small" type="warning">通道</el-tag>
                      <el-tag v-else size="small">{{ device.device_type || 'Other' }}</el-tag>
                    </el-descriptions-item>
                    <el-descriptions-item label="通道ID" v-if="device.channel_id">{{ device.channel_id }}</el-descriptions-item>
                    <el-descriptions-item label="父设备" v-if="device.parent_device_tag">{{ device.parent_device_tag }}</el-descriptions-item>
                    <el-descriptions-item label="观看次数">{{ device.view_count }}</el-descriptions-item>
                    <el-descriptions-item label="媒体服务器">{{ device.media_server_name || '-' }}</el-descriptions-item>
                    <el-descriptions-item label="应用名称">{{ device.app || '-' }}</el-descriptions-item>
                    <el-descriptions-item label="流标识">{{ device.stream_key || '-' }}</el-descriptions-item>
                    <el-descriptions-item label="拉流状态">{{ activeStream ? '已启动' : '未启动' }}</el-descriptions-item>
                    <el-descriptions-item label="维护模式">
                      <el-switch
                        v-model="maintenanceMode"
                        active-text="开启"
                        inactive-text="关闭"
                        :loading="maintenanceLoading"
                        @change="toggleMaintenance"
                      />
                    </el-descriptions-item>
                    <el-descriptions-item label="主机地址">{{ device.host || '-' }}:{{ device.port || '-' }}</el-descriptions-item>
                    <el-descriptions-item label="创建时间">{{ formatDate(device.created_at) }}</el-descriptions-item>
                    <el-descriptions-item label="最后在线">{{ device.last_seen ? formatDate(device.last_seen) : '从未' }}</el-descriptions-item>
                    <el-descriptions-item label="所属分组">{{ device.group_id || '-' }}</el-descriptions-item>
                  </el-descriptions>
                </el-card>
              </el-col>
            </el-row>

            <el-card style="margin-top: 12px">
              <template #header>
                <div style="display: flex; align-items: center; justify-content: space-between">
                  <div class="ptz-hd-title" style="font-size: 18px; font-weight: 900; letter-spacing: 4px; color: var(--ptz-accent, #409eff); text-shadow: 0 0 14px var(--ptz-accent, #409eff);">PTZ</div>
                  <div style="display: flex; gap: 8px">
                    <div class="ptz-hd-signal" style="display: flex; align-items: center; gap: 5px; font-size: 8px; letter-spacing: 1px; color: var(--text-muted, #909399)">
                      <div class="ptz-hd-dot" :class="isPtzDevice ? (device.status?.toLowerCase() === 'online' ? 'sig-ok' : 'sig-no') : 'sig-off'" />
                      <span>{{ isPtzDevice ? (device.status?.toLowerCase() === 'online' ? '在线' : '离线') : '无云台' }}</span>
                    </div>
                  </div>
                </div>
              </template>
              <el-empty v-if="!isPtzDevice" description="该设备不支持云台控制" :image-size="60" />
              <template v-else>
                <div class="ptz-ch" style="padding: 8px 12px; border-bottom: 1px solid var(--border, #dcdfe6); background: rgba(64, 158, 255, 0.06); margin: -12px -12px 12px -12px; border-radius: 0; border-top: none;">
                  <div style="display: flex; align-items: center; gap: 6px; font-size: 8px; color: var(--text-muted, #909399)">
                    <span class="ptz-ch-tag" :class="isPtzDevice ? 'ptz-tag-ok' : 'ptz-tag-off'" style="font-size: 8px; letter-spacing: 1px; padding: 1px 5px; border-radius: 2px; border: 1px solid;">{{ device.protocol }}</span>
                    <span v-if="ptzStatus">P={{ ptzStatus.position_pan?.toFixed(2) || '?' }} T={{ ptzStatus.position_tilt?.toFixed(2) || '?' }} Z={{ ptzStatus.position_zoom?.toFixed(2) || '?' }}</span>
                    <span style="margin-left: auto; font-size: 9px;" :style="{ color: ptzStatus?.moving ? 'var(--el-color-warning, #e6a23c)' : 'var(--el-color-success, #67c23a)' }">{{ ptzStatus?.moving ? '运动中' : '空闲' }}</span>
                    <el-button v-if="isGb28181" size="small" :type="isTalking ? 'danger' : 'primary'" style="padding: 2px 8px; font-size: 9px;" @click="toggleVoiceTalk" :disabled="device.status?.toLowerCase() !== 'online'">
                      {{ isTalking ? '结束对讲' : '语音对讲' }}
                    </el-button>
                  </div>
                </div>

                <el-row :gutter="24">
                  <el-col :span="8">
                    <div class="ptz-zone" style="display: flex; flex-direction: column; align-items: center; gap: 12px;">
                      <div class="ptz-crosshair" style="position: relative; width: 120px; height: 120px; display: grid; grid-template-columns: repeat(3, 40px); grid-template-rows: repeat(3, 40px); gap: 0;">
                        <div style="position: absolute; top: 50%; left: 0; right: 0; height: 1px; background: var(--border, #dcdfe6); pointer-events: none;"></div>
                        <div style="position: absolute; top: 0; bottom: 0; left: 50%; width: 1px; background: var(--border, #dcdfe6); pointer-events: none;"></div>
                        <button class="ptz-dir-btn ptz-dir-up" :class="{ on: ptzActiveDir === 'up' }" :disabled="!isPtzDevice"
                          @mousedown.prevent="ptzActiveDir = 'up'; ptzCmd('up')"
                          @mouseup="ptzActiveDir = null; ptzStop()"
                          @mouseleave="ptzActiveDir === 'up' && ptzStop()">
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 19V5M5 12l7-7 7 7"/></svg>
                        </button>
                        <button class="ptz-dir-btn ptz-dir-lf" :class="{ on: ptzActiveDir === 'left' }" :disabled="!isPtzDevice"
                          @mousedown.prevent="ptzActiveDir = 'left'; ptzCmd('left')"
                          @mouseup="ptzActiveDir = null; ptzStop()"
                          @mouseleave="ptzActiveDir === 'left' && ptzStop()">
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M19 12H5M12 5l-7 7 7 7"/></svg>
                        </button>
                        <button class="ptz-stop-btn" :disabled="!isPtzDevice" @click="ptzStop()">
                          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>
                        </button>
                        <button class="ptz-dir-btn ptz-dir-rt" :class="{ on: ptzActiveDir === 'right' }" :disabled="!isPtzDevice"
                          @mousedown.prevent="ptzActiveDir = 'right'; ptzCmd('right')"
                          @mouseup="ptzActiveDir = null; ptzStop()"
                          @mouseleave="ptzActiveDir === 'right' && ptzStop()">
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M5 12h14M12 5l7 7-7 7"/></svg>
                        </button>
                        <button class="ptz-dir-btn ptz-dir-dn" :class="{ on: ptzActiveDir === 'down' }" :disabled="!isPtzDevice"
                          @mousedown.prevent="ptzActiveDir = 'down'; ptzCmd('down')"
                          @mouseup="ptzActiveDir = null; ptzStop()"
                          @mouseleave="ptzActiveDir === 'down' && ptzStop()">
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 5v14M5 12l7 7 7-7"/></svg>
                        </button>
                      </div>
                      <div style="width: 120px; text-align: center;">
                        <el-text type="info" size="small" style="font-size: 9px; letter-spacing: 1px">速度: {{ ptzSpeed }}</el-text>
                        <div style="position: relative; height: 4px; background: var(--bg-surface, #f5f7fa); border-radius: 2px; border: 1px solid var(--border, #dcdfe6); margin-top: 4px; overflow: visible;">
                          <div style="position: absolute; left: 0; top: 0; bottom: 0; border-radius: 2px; background: linear-gradient(90deg, var(--el-color-primary, #409eff), var(--el-color-primary, #409eff); transition: width 0.1s;" :style="{ width: ptzSpeed + '%' }"></div>
                        </div>
                        <input type="range" min="1" max="100" v-model="ptzSpeed" :disabled="!isPtzDevice" style="position: absolute; inset: -6px 0; width: 100%; height: 16px; opacity: 0; cursor: pointer; margin-top: -12px;" />
                      </div>
                      <div class="ptz-ft" style="font-size: 9px; letter-spacing: 1px; transition: color 0.15s; text-align: center; width: 100%;" :style="{ color: ptzActiveDir ? 'var(--el-color-primary, #409eff)' : 'var(--text-muted, #909399)' }">
                        {{ ptzActiveDir ? { up:'上移中', down:'下移中', left:'左移中', right:'右移中', zoom_in:'放大中', zoom_out:'缩小中', focus_in:'近焦中', focus_out:'远焦中' }[ptzActiveDir] : '— 就绪 —' }}
                      </div>
                    </div>
                  </el-col>

                  <el-col :span="8">
                    <div class="ptz-zone" style="display: flex; flex-direction: column; gap: 12px;">
                      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
                        <div>
                          <div style="font-size: 8px; letter-spacing: 1.5px; color: var(--text-muted, #909399); margin-bottom: 4px; text-align: center;">ZOOM</div>
                          <div style="display: flex; gap: 4px;">
                            <button class="ptz-opt-btn" :class="{ on: ptzActiveDir === 'zoom_in' }" :disabled="!isPtzDevice"
                              @mousedown.prevent="ptzActiveDir = 'zoom_in'; ptzCmd('zoom_in')"
                              @mouseup="ptzActiveDir = null; ptzStop()"
                              @mouseleave="ptzActiveDir === 'zoom_in' && ptzStop()">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="11" cy="11" r="8"/><path d="M21 21l-4.35-4.35M11 8v6M8 11h6"/></svg>
                            </button>
                            <button class="ptz-opt-btn" :class="{ on: ptzActiveDir === 'zoom_out' }" :disabled="!isPtzDevice"
                              @mousedown.prevent="ptzActiveDir = 'zoom_out'; ptzCmd('zoom_out')"
                              @mouseup="ptzActiveDir = null; ptzStop()"
                              @mouseleave="ptzActiveDir === 'zoom_out' && ptzStop()">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="11" cy="11" r="8"/><path d="M21 21l-4.35-4.35M8 11h6"/></svg>
                            </button>
                          </div>
                        </div>
                        <div>
                          <div style="font-size: 8px; letter-spacing: 1.5px; color: var(--text-muted, #909399); margin-bottom: 4px; text-align: center;">FOCUS</div>
                          <div style="display: flex; gap: 4px;">
                            <button class="ptz-opt-btn" :class="{ on: ptzActiveDir === 'focus_in' }" :disabled="!isPtzDevice"
                              @mousedown.prevent="ptzActiveDir = 'focus_in'; ptzCmd('focus_in')"
                              @mouseup="ptzActiveDir = null; ptzStop()"
                              @mouseleave="ptzActiveDir === 'focus_in' && ptzStop()">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="3"/><path d="M3 12h3M18 12h3M12 3v3M12 18v3"/></svg>
                            </button>
                            <button class="ptz-opt-btn" :class="{ on: ptzActiveDir === 'focus_out' }" :disabled="!isPtzDevice"
                              @mousedown.prevent="ptzActiveDir = 'focus_out'; ptzCmd('focus_out')"
                              @mouseup="ptzActiveDir = null; ptzStop()"
                              @mouseleave="ptzActiveDir === 'focus_out' && ptzStop()">
                              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="3"/><path d="M3 12h3M18 12h3M12 3v3M12 18v3"/></svg>
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </el-col>

                  <el-col :span="8">
                    <div class="ptz-zone" style="display: flex; flex-direction: column; gap: 8px;">
                      <div style="display: flex; align-items: center; justify-content: space-between;">
                        <el-text type="info" size="small" style="font-size: 9px; letter-spacing: 1.5px">预制位 ({{ presets.length }}个)</el-text>
                        <el-button size="small" style="padding: 2px 8px; font-size: 10px;" @click="showPresetDialog = true" :disabled="!isPtzDevice">+ 保存</el-button>
                        <el-button size="small" style="padding: 2px 8px; font-size: 10px;" :icon="Refresh" @click="refreshPtzStatus" :disabled="!isPtzDevice" />
                      </div>
                      <div style="display: flex; flex-direction: column; gap: 2px; max-height: 160px; overflow-y: auto;">
                        <div v-if="presets.length === 0" style="font-size: 8px; letter-spacing: 1.5px; color: var(--text-muted, #909399); text-align: center; padding: 10px 0;">暂无预制位</div>
                        <div
                          v-for="(p, i) in presets" :key="p.token"
                          style="display: flex; align-items: center; gap: 6px; padding: 4px 5px; border-radius: 3px; cursor: pointer; border-bottom: 1px solid var(--border, #dcdfe6); transition: background 0.1s;"
                          :class="{ disabled: !isPtzDevice }"
                          @click="isPtzDevice && gotoPreset(p)"
                          @mouseenter="(e) => e.currentTarget.style.background = isPtzDevice ? 'rgba(64, 158, 255, 0.06)' : ''"
                          @mouseleave="(e) => e.currentTarget.style.background = ''"
                        >
                          <span style="font-size: 8px; color: var(--text-muted, #909399); letter-spacing: 1px; min-width: 16px;">{{ String(i + 1).padStart(2, '0') }}</span>
                          <el-text v-if="editingPresetToken !== p.token" truncated style="flex: 1; font-size: 10px; color: var(--text-primary, #303133); letter-spacing: .3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;" @dblclick.stop="startEditPreset(p)">{{ p.name }}</el-text>
                          <el-input v-else size="small" style="flex: 1;" :ref="el => presetInputRefs[p.token] = el" v-model="editingPresetName" @keyup.enter="savePresetRename(p)" @keyup.escape="cancelEditPreset" @blur="cancelEditPreset" @click.stop />
                          <button v-if="editingPresetToken !== p.token" style="width: 18px; height: 18px; flex-shrink: 0; border: none; background: transparent; color: var(--text-muted, #909399); display: flex; align-items: center; justify-content: center; cursor: pointer; border-radius: 2px; transition: color 0.1s; padding: 0;" :disabled="!isPtzDevice" @click.stop="removePreset(p)" @mouseenter="(e) => e.currentTarget.style.color = 'var(--el-color-danger, #f56c6c)'" @mouseleave="(e) => e.currentTarget.style.color = 'var(--text-muted, #909399)'">
                            <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M18 6L6 18M6 6l12 12"/></svg>
                          </button>
                          <button v-else style="width: 18px; height: 18px; flex-shrink: 0; border: none; background: transparent; color: var(--el-color-success, #67c23a); display: flex; align-items: center; justify-content: center; cursor: pointer; padding: 0;" @click.stop="savePresetRename(p)">
                            <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M20 6L9 17l-5-5"/></svg>
                          </button>
                        </div>
                      </div>
                    </div>
                  </el-col>
                </el-row>
              </template>
            </el-card>

            <el-card style="margin-top: 12px">
              <template #header>
                <div style="display: flex; align-items: center; justify-content: space-between">
                  <span>播放链接</span>
                  <el-button size="small" @click="fetchPlayLinks">获取播放链接</el-button>
                </div>
              </template>
              <el-skeleton animated :rows="2" :loading="loadingLinks">
                <template #default>
                  <el-descriptions v-if="playLinks" :column="1" border size="small">
                    <el-descriptions-item label="Token">
                      <div style="display: flex; gap: 8px; align-items: center">
                        <el-text style="font-family: monospace">{{ playLinks.token }}</el-text>
                        <el-button size="small" @click="copyLink(playLinks.token)">复制</el-button>
                      </div>
                    </el-descriptions-item>
                    <el-descriptions-item label="过期时间">{{ formatExpiry(playLinks.expires_at) }}</el-descriptions-item>
                  </el-descriptions>
                  <el-empty v-else description="点击「获取播放链接」以生成播放地址" />
                </template>
              </el-skeleton>
            </el-card>

            <el-card style="margin-top: 12px" v-if="isGb28181 || isOnvif">
              <template #header>
                <div style="display: flex; align-items: center; justify-content: space-between">
                  <span>设备配置</span>
                  <el-button size="small" @click="queryDeviceConfig">刷新配置</el-button>
                </div>
              </template>
              <el-descriptions :column="2" border size="small">
                <el-descriptions-item label="查询类型">
                  <el-select v-model="configType" size="small" style="width: 120px">
                    <el-option label="基本参数" value="BasicParam" />
                    <el-option label="网络参数" value="NetworkParam" />
                    <el-option label="视频参数" value="VideoParam" />
                    <el-option label="视频源" value="VideoSrcParam" />
                  </el-select>
                </el-descriptions-item>
                <el-descriptions-item label="操作">
                  <el-button size="small" type="primary" @click="queryDeviceConfig">查询配置</el-button>
                </el-descriptions-item>
              </el-descriptions>
              <el-skeleton animated :rows="3" :loading="configLoading" style="margin-top: 12px">
                <template #default>
                  <el-alert v-if="configError" :title="configError" type="error" :closable="false" style="margin-bottom: 12px" />
                  <el-input v-else-if="configData" v-model="configData" type="textarea" :rows="6" readonly style="font-family: monospace; font-size: 12px" />
                  <el-empty v-else description="点击「查询配置」获取设备配置信息" />
                </template>
              </el-skeleton>
            </el-card>
          </div>
        </template>
      </el-skeleton>
    </div>

    <el-dialog v-model="showPresetDialog" title="保存预制位" width="400px" :close-on-click-modal="false">
      <el-form-item label="预制位名称">
        <el-input v-model="newPresetName" placeholder="例如: 大门入口" />
      </el-form-item>
      <template #footer>
        <el-button @click="showPresetDialog = false">取消</el-button>
        <el-button type="primary" :disabled="!newPresetName.trim()" @click="savePreset">保存当前PTZ位置</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="showPullModal" title="启动拉流" width="500px" :close-on-click-modal="false">
      <el-form-item label="拉流地址">
        <el-input v-model="pullForm.rtspUrl" :placeholder="defaultPullUrl || 'rtsp://camera-ip:554/stream'" />
      </el-form-item>
      <el-alert v-if="pullError" :title="pullError" type="error" :closable="false" style="margin-top: 8px" />
      <template #footer>
        <el-button @click="showPullModal = false">取消</el-button>
        <el-button type="primary" :loading="pullLoading" @click="startPull">{{ pullLoading ? '启动中...' : '启动' }}</el-button>
      </template>
    </el-dialog>

    <el-affix position="bottom" :offset="20" v-if="copied">
      <el-alert title="已复制到剪贴板" type="success" :closable="false" show-icon />
    </el-affix>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, watch, nextTick } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage } from 'element-plus'
import { ArrowLeft, Refresh, Edit, VideoPlay, Close, Plus, CopyDocument } from '@element-plus/icons-vue'
import VideoPlayer from '../components/VideoPlayer.vue'
import { useDeviceStore } from '../stores/deviceStore'
import { useStreamStore } from '../stores/streamStore'
import { useMediaServerStore } from '../stores/mediaServerStore'
import { useToast } from '../composables/useToast'
import { request } from '../utils/request'
import * as streamsApi from '../api/streams'

const route = useRoute()
const deviceStore = useDeviceStore()
const streamStore = useStreamStore()
const mediaServerStore = useMediaServerStore()
const toast = useToast()

const device = ref(null)
const playLinks = ref(null)
const loading = ref(false)
const loadingLinks = ref(false)
const copied = ref(false)
const activePlayerTab = ref('player')
const showEditModal = ref(false)
const ptzSpeed = ref(50)
const ptzActiveDir = ref(null)
let ptzTimeout = null
const presets = ref([])
const showPresetDialog = ref(false)
const newPresetName = ref('')
const ptzStatus = ref(null)
const showPullModal = ref(false)
const activeStream = ref(null)
const pullForm = ref({ rtspUrl: '' })
const pullLoading = ref(false)
const pullError = ref('')
const maintenanceMode = ref(false)
const maintenanceLoading = ref(false)
let copiedTimer = null
const editingPresetToken = ref(null)
const editingPresetName = ref('')
const presetInputRefs = ref({})
const servers = ref([])
const configType = ref('BasicParam')
const configData = ref('')
const configLoading = ref(false)
const configError = ref('')
const isTalking = ref(false)
const audioWs = ref(null)
const mediaRecorder = ref(null)

const proto = computed(() => (device.value?.protocol || '').toLowerCase())
const isPtzDevice = computed(() => proto.value === 'onvif' || proto.value === 'gb28181')
const isRtspPull = computed(() => proto.value === 'rtsp' && ((device.value?.extended?.rtsp_mode || device.value?.rtsp_mode) || 'pull') === 'pull')
const isRtspPush = computed(() => proto.value === 'rtsp' && ((device.value?.extended?.rtsp_mode || device.value?.rtsp_mode) || 'pull') === 'push')
const isRtmpPush = computed(() => proto.value === 'rtmp')
const isOnvif = computed(() => proto.value === 'onvif')
const isGb28181 = computed(() => proto.value === 'gb28181')

const currentServer = computed(() => {
  if (!device.value?.media_server_name) return null
  return servers.value.find(s => s.name === device.value.media_server_name) || null
})

const mediaServerPushUrl = computed(() => {
  const s = currentServer.value
  if (!s || !device.value?.stream_key) return ''
  const addr = (s.url || '').replace(/\/$/, '')
  const port = s.protocol_ports?.rtsp || 554
  const app = device.value.app || 'live'
  return `${addr}:${port}/${app}/${device.value.stream_key}`
})

const rtmpPushUrl = computed(() => {
  const s = currentServer.value
  if (!s || !device.value?.stream_key) return ''
  const addr = (s.url || '').replace(/\/$/, '')
  const port = s.protocol_ports?.rtmp || 1935
  return `${addr}:${port}/${device.value.stream_key}`
})

const defaultPullUrl = computed(() => {
  if (!device.value) return ''
  if (device.value.pull_urls?.length) return device.value.pull_urls[0].url
  if (isRtspPull.value && (device.value.extended?.rtsp_full_url || device.value.rtsp_full_url)) return device.value.extended?.rtsp_full_url || device.value.rtsp_full_url
  if (device.value.host) return `rtsp://${device.value.host}:${device.value.port || 554}/stream`
  return ''
})

const selectedLink = ref(null)

const playerTabs = [
  { label: '播放器', value: 'player' },
  { label: '链接列表', value: 'links' },
]

const allPlayLinks = computed(() => {
  if (!playLinks.value) return []
  return [
    { key: 'rtsp_signaling', label: 'RTSP (信令)', tag: 'RTSP', tagType: 'primary', url: playLinks.value.rtsp_signaling },
    { key: 'rtsp_media', label: 'RTSP (直连)', tag: 'RTSP', tagType: 'primary', url: playLinks.value.rtsp_media },
    { key: 'flv', label: 'HTTP-FLV', tag: 'FLV', tagType: 'info', url: playLinks.value.flv },
    { key: 'web_flv', label: 'Web-FLV', tag: 'FLV', tagType: 'info', url: playLinks.value.web_flv },
    { key: 'hls', label: 'HLS', tag: 'HLS', tagType: 'success', url: playLinks.value.hls },
    { key: 'webrtc', label: 'WebRTC', tag: 'RTC', tagType: 'warning', url: playLinks.value.webrtc },
  ]
})

const availablePlayLinks = computed(() => allPlayLinks.value.filter(l => l.url))

const activeStreamUrl = computed(() => {
  if (selectedLink.value) return selectedLink.value.url
  if (!playLinks.value) return null
  return playLinks.value.hls || playLinks.value.web_flv || playLinks.value.rtsp_signaling || null
})

const activeStreamType = computed(() => {
  if (selectedLink.value) return selectedLink.value.type
  if (!playLinks.value) return 'auto'
  if (playLinks.value.hls) return 'hls'
  if (playLinks.value.web_flv) {
    const url = playLinks.value.web_flv
    if (url.startsWith('ws://') || url.startsWith('wss://')) return 'wsflv'
    return 'flv'
  }
  if (playLinks.value.flv) return 'flv'
  return 'auto'
})

onMounted(async () => {
  await Promise.all([fetchDevice(), checkActiveStream(), mediaServerStore.fetchServers()])
  servers.value = mediaServerStore.servers
})

watch(device, async (d) => {
  if (d && isPtzDevice.value) { await fetchPresets(); await refreshPtzStatus() }
  if (d) { maintenanceMode.value = d.status === 'Maintaining' }
}, { immediate: true })

async function startPull() {
  if (!device.value) return
  const rtspUrl = pullForm.value.rtspUrl || defaultPullUrl.value
  if (!rtspUrl) return
  pullLoading.value = true; pullError.value = ''
  try {
    const streamInfo = await streamStore.startStream(device.value.id, rtspUrl)
    if (streamInfo) activeStream.value = streamInfo
    showPullModal.value = false; pullForm.value = { rtspUrl: '' }
  } catch (e) { pullError.value = e.response?.data?.message || e.message || '启动失败' }
  finally { pullLoading.value = false }
}

async function stopPull() {
  if (!activeStream.value) return
  try {
    const streams = streamStore.streams
    const matched = streams.find(s => s.stream_key === activeStream.value.stream_key)
    if (matched) await streamStore.stopStream(matched.id)
    activeStream.value = null
  } catch (e) { console.error('Failed to stop pull:', e) }
}

async function toggleMaintenanceAndStop() {
  maintenanceLoading.value = true
  try {
    if (activeStream.value) {
      await streamStore.stopStream(activeStream.value.id)
      activeStream.value = null
    }
    await deviceStore.updateDevice(device.value.id, { status: 'Maintaining' })
    device.value.status = 'Maintaining'
    maintenanceMode.value = true
    ElMessage.success('已开启维护模式并停止拉流')
  } catch (e) {
    ElMessage.error('操作失败: ' + (e.message || e))
  } finally {
    maintenanceLoading.value = false
  }
}

async function toggleMaintenance(val) {
  if (!device.value) return
  maintenanceLoading.value = true
  try {
    await deviceStore.updateDevice(device.value.id, { status: val ? 'Maintaining' : 'Online' })
    device.value.status = val ? 'Maintaining' : 'Online'
    ElMessage.success(val ? '已开启维护模式' : '已关闭维护模式')
  } catch (e) {
    maintenanceMode.value = !val
    ElMessage.error('设置维护模式失败: ' + (e.message || e))
  } finally {
    maintenanceLoading.value = false
  }
}

async function checkActiveStream() {
  if (device.value) {
    try {
      const streams = await streamsApi.getStreamsByDevice(device.value.id)
      const found = streams.length > 0 ? streams[0] : null
      if (found) {
        try {
          const online = await streamsApi.isStreamOnline(found.id)
          activeStream.value = online ? found : null
        } catch (e) {
          console.error('Failed to check stream online status:', e)
          activeStream.value = found
        }
      } else {
        activeStream.value = null
      }
    } catch (e) {
      console.error('Failed to fetch streams:', e)
      activeStream.value = null
    }
  }
}

async function fetchDevice() { loading.value = true; try { device.value = await deviceStore.fetchDevice(route.params.id) } finally { loading.value = false } }
async function fetchPlayLinks() { loadingLinks.value = true; try { playLinks.value = await deviceStore.getPlayLinks(route.params.id); selectedLink.value = null; activePlayerTab.value = 'player' } finally { loadingLinks.value = false } }

async function queryDeviceConfig() {
  configLoading.value = true
  configError.value = ''
  configData.value = ''
  try {
    const res = await request.get(`/devices/${route.params.id}/config`, { params: { type: configType.value } })
    configData.value = res.data?.data || res.data || '无配置数据'
  } catch (e) {
    configError.value = '查询失败: ' + (e.message || e)
  } finally {
    configLoading.value = false
  }
}

function playStream(link) {
  let type = 'auto'
  if (link.key === 'hls') type = 'hls'
  else if (link.key === 'web_flv') type = 'wsflv'
  else if (link.key === 'flv') type = 'flv'
  selectedLink.value = { url: link.url, type }
  activePlayerTab.value = 'player'
}

async function openPreview(url) {
  try {
    await navigator.clipboard.writeText(url)
    ElMessage.success('RTSP 地址已复制到剪贴板')
  } catch {
    ElMessage.error('复制失败，请手动复制')
  }
}

async function copyLink(text) {
  try { await navigator.clipboard.writeText(text) } catch {
    const el = document.createElement('textarea'); el.value = text; document.body.appendChild(el); el.select(); document.execCommand('copy'); document.body.removeChild(el)
  }
  copied.value = true; clearTimeout(copiedTimer); copiedTimer = setTimeout(() => { copied.value = false }, 2000)
}

function formatDate(dateStr) { if (!dateStr) return '-'; return new Date(dateStr).toLocaleString('zh-CN', { hour12: false }) }
function formatExpiry(ts) { if (!ts) return '-'; return new Date(ts * 1000).toLocaleString('zh-CN', { hour12: false }) }
function getStatusClass(status) {
  if (!status) return 'offline'
  const s = status.toLowerCase()
  if (s === 'online') return 'online'
  if (s === 'maintaining') return 'maintaining'
  if (s === 'error') return 'error'
  return 'offline'
}

async function ptzCmd(cmd) {
  clearTimeout(ptzTimeout)
  try { await deviceStore.ptzControl(route.params.id, cmd, ptzSpeed.value) } catch (e) { toast.error('PTZ控制失败: ' + (e.message || e)) }
}

function ptzStop() {
  clearTimeout(ptzTimeout)
  ptzTimeout = setTimeout(async () => { try { await deviceStore.ptzControl(route.params.id, 'stop', ptzSpeed.value) } catch (e) {} }, 200)
}

async function toggleVoiceTalk() {
  if (isTalking.value) {
    stopVoiceTalk()
  } else {
    startVoiceTalk()
  }
}

async function startVoiceTalk() {
  try {
    const wsUrl = `ws://${window.location.host}/ws/audio-talk/${route.params.id}`
    audioWs.value = new WebSocket(wsUrl)
    
    audioWs.value.onopen = async () => {
      console.log('[VoiceTalk] WebSocket connected')
      isTalking.value = true
      
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
        mediaRecorder.value = new MediaRecorder(stream, { mimeType: 'audio/webm' })
        
        mediaRecorder.value.ondataavailable = (e) => {
          if (e.data.size > 0 && audioWs.value?.readyState === WebSocket.OPEN) {
            audioWs.value.send(e.data)
          }
        }
        
        mediaRecorder.value.start(100)
      } catch (e) {
        console.error('[VoiceTalk] Failed to get microphone:', e)
        toast.error('无法访问麦克风')
        stopVoiceTalk()
      }
    }
    
    audioWs.value.onclose = () => {
      console.log('[VoiceTalk] WebSocket closed')
      stopVoiceTalk()
    }
    
    audioWs.value.onerror = (e) => {
      console.error('[VoiceTalk] WebSocket error:', e)
      toast.error('语音对讲连接失败')
      stopVoiceTalk()
    }
  } catch (e) {
    console.error('[VoiceTalk] Failed to start:', e)
    toast.error('启动语音对讲失败')
  }
}

function stopVoiceTalk() {
  if (mediaRecorder.value && mediaRecorder.value.state !== 'inactive') {
    mediaRecorder.value.stop()
    mediaRecorder.value.stream.getTracks().forEach(track => track.stop())
    mediaRecorder.value = null
  }
  
  if (audioWs.value) {
    audioWs.value.close()
    audioWs.value = null
  }
  
  isTalking.value = false
}

async function fetchPresets() {
  try {
    const res = await request.get(`/devices/${route.params.id}/ptz/presets`)
    presets.value = res.data.data?.presets || []
  } catch (e) { presets.value = [] }
}

async function refreshPtzStatus() {
  try {
    const res = await request.get(`/devices/${route.params.id}/ptz/status`)
    ptzStatus.value = res.data.data?.status || null
  } catch (e) { ptzStatus.value = null }
}

async function savePreset() {
  if (!newPresetName.value.trim()) return
  try {
    await request.post(`/devices/${route.params.id}/ptz/presets`, { name: newPresetName.value.trim() })
    await fetchPresets()
    showPresetDialog.value = false
    newPresetName.value = ''
    ElMessage.success('预制位已保存')
  } catch (e) { toast.error('保存预制位失败: ' + (e.message || e)) }
}

async function gotoPreset(preset) {
  try { await deviceStore.ptzControl(route.params.id, 'goto_preset', ptzSpeed.value, preset.token) } catch (e) { toast.error('调用预制位失败: ' + (e.message || e)) }
}

async function removePreset(preset) {
  try {
    await request.delete(`/devices/${route.params.id}/ptz/presets/${preset.token}`)
    await fetchPresets()
  } catch (e) { toast.error('删除预制位失败: ' + (e.message || e)) }
}

function startEditPreset(preset) {
  editingPresetToken.value = preset.token
  editingPresetName.value = preset.name
  nextTick(() => {
    const el = presetInputRefs.value[preset.token]
    if (el?.$el) el.$el.focus()
    else if (el?.focus) el.focus()
  })
}

async function savePresetRename(preset) {
  if (!editingPresetName.value.trim()) return
  try {
    await request.put(`/devices/${route.params.id}/ptz/presets/${preset.token}`, { name: editingPresetName.value.trim() })
    await fetchPresets()
    editingPresetToken.value = null
    ElMessage.success('预制位已重命名')
  } catch (e) { toast.error('重命名失败: ' + (e.message || e)) }
}

function cancelEditPreset() {
  editingPresetToken.value = null
  editingPresetName.value = ''
}
</script>

<style scoped>
.ptz-dir-btn {
  width: 38px; height: 38px;
  border: 1px solid var(--border, #dcdfe6); background: var(--bg-surface, #f5f7fa);
  border-radius: 6px; color: var(--text-primary, #303133);
  display: flex; align-items: center; justify-content: center;
  cursor: pointer; transition: all 0.1s; outline: none; padding: 0;
}
.ptz-dir-btn:hover:not(:disabled) {
  border-color: var(--el-color-primary, #409eff); color: var(--el-color-primary, #409eff);
  background: rgba(64, 158, 255, 0.06);
}
.ptz-dir-btn.on {
  border-color: var(--el-color-primary, #409eff); color: var(--el-color-primary, #409eff);
  background: rgba(64, 158, 255, 0.06);
}
.ptz-dir-btn:disabled { opacity: .3; cursor: not-allowed; }
.ptz-dir-up { grid-column: 2; grid-row: 1; justify-self: center; }
.ptz-dir-lf { grid-column: 1; grid-row: 2; justify-self: center; }
.ptz-stop-btn {
  grid-column: 2; grid-row: 2; justify-self: center;
  width: 34px; height: 34px;
  border: 1px solid rgba(239, 68, 68, 0.25); background: rgba(239, 68, 68, 0.06);
  border-radius: 50%; color: var(--el-color-danger, #f56c6c);
  display: flex; align-items: center; justify-content: center;
  cursor: pointer; transition: all 0.1s; outline: none; padding: 0;
}
.ptz-stop-btn:hover:not(:disabled) {
  border-color: var(--el-color-danger, #f56c6c); background: rgba(239, 68, 68, 0.1);
}
.ptz-stop-btn:disabled { opacity: .3; cursor: not-allowed; }
.ptz-dir-rt { grid-column: 3; grid-row: 2; justify-self: center; }
.ptz-dir-dn { grid-column: 2; grid-row: 3; justify-self: center; }
.ptz-opt-btn {
  flex: 1; height: 30px;
  border: 1px solid var(--border, #dcdfe6); background: var(--bg-surface, #f5f7fa);
  border-radius: 4px; color: var(--text-muted, #909399);
  display: flex; align-items: center; justify-content: center; gap: 3px;
  cursor: pointer; transition: all 0.1s; font-size: 9px; letter-spacing: .5px;
  outline: none; padding: 0;
}
.ptz-opt-btn:hover:not(:disabled) {
  border-color: var(--el-color-primary, #409eff); color: var(--el-color-primary, #409eff);
  background: rgba(64, 158, 255, 0.06);
}
.ptz-opt-btn.on {
  border-color: var(--el-color-primary, #409eff); color: var(--el-color-primary, #409eff);
  background: rgba(64, 158, 255, 0.06);
}
.ptz-opt-btn:disabled { opacity: .3; cursor: not-allowed; }
.ptz-hd-dot {
  width: 7px; height: 7px; border-radius: 50%;
  background: var(--text-muted, #909399);
  transition: background 0.3s, box-shadow 0.3s;
}
.ptz-hd-dot.sig-ok {
  background: var(--el-color-success, #67c23a);
  animation: ptz-pulse 2s infinite;
}
.ptz-hd-dot.sig-no { background: var(--el-color-warning, #e6a23c); }
.ptz-hd-dot.sig-off { background: var(--text-muted, #909399); }
@keyframes ptz-pulse {
  0%,100% { opacity: 1; }
  50% { opacity: .4; }
}
</style>
