import { defineStore } from 'pinia'
import { ElMessage } from 'element-plus'
import * as api from '../api/recordings'

export const useRecordingStore = defineStore('recordings', {
  state: () => ({
    recordings: [],
    stats: null,
    loading: false,
    error: null,
    total: 0,
    policies: [],
    policyLoading: false,
    policyTotal: 0,
    recordingFiles: [],
    filesLoading: false,
    filesTotal: 0,
    filesPage: 1,
    filesLimit: 50,
  }),

  actions: {
    async fetchRecordings(params = {}) {
      this.loading = true
      this.error = null
      try {
        const data = await api.getRecordings(params)
        this.recordings = data.items || data
        this.total = data.total ?? this.recordings.length
        return data
      } catch (e) {
        this.error = e.message
        throw e
      } finally {
        this.loading = false
      }
    },

    async fetchStats() {
      this.stats = await api.getRecordingStats()
    },

    async createRecording(data) {
      const recording = await api.createRecording(data)
      this.recordings.unshift(recording)
      this.total++
      ElMessage.success('录像任务已创建')
      return recording
    },

    async startRecording(id) {
      const recording = await api.startRecording(id)
      const idx = this.recordings.findIndex(r => r.id === id)
      if (idx !== -1) this.recordings[idx] = recording
      ElMessage.success('录像已开始')
      return recording
    },

    async stopRecording(id) {
      const recording = await api.stopRecording(id)
      const idx = this.recordings.findIndex(r => r.id === id)
      if (idx !== -1) this.recordings[idx] = recording
      ElMessage.success('录像已停止')
      return recording
    },

    async pauseRecording(id) {
      const recording = await api.pauseRecording(id)
      const idx = this.recordings.findIndex(r => r.id === id)
      if (idx !== -1) this.recordings[idx] = recording
      ElMessage.success('录像已暂停')
      return recording
    },

    async resumeRecording(id) {
      const recording = await api.resumeRecording(id)
      const idx = this.recordings.findIndex(r => r.id === id)
      if (idx !== -1) this.recordings[idx] = recording
      ElMessage.success('录像已继续')
      return recording
    },

    async deleteRecording(id) {
      await api.deleteRecording(id)
      this.recordings = this.recordings.filter(r => r.id !== id)
      this.total--
      ElMessage.success('录像任务已删除')
    },

    async fetchFiles(id) {
      return await api.getRecordingFilesById(id)
    },

    async fetchRecordingFiles(params) {
      this.filesLoading = true
      try {
        const data = await api.getRecordingFilesWithParams(params)
        if (params.page && params.page > 1) {
          this.recordingFiles = [...this.recordingFiles, ...(data.files || [])]
        } else {
          this.recordingFiles = data.files || []
        }
        this.filesTotal = data.total || 0
        this.filesPage = data.page || 1
        this.filesLimit = data.limit || 50
        return data
      } finally {
        this.filesLoading = false
      }
    },

    async getRecordingFilePlayUrl(fileId) {
      return await api.getRecordingFilePlayUrl(fileId)
    },

    async fetchPolicies(params = {}) {
      this.policyLoading = true
      try {
        const data = await api.getRecordingPolicies(params)
        this.policies = data.items || data
        this.policyTotal = data.total ?? this.policies.length
        return data
      } finally {
        this.policyLoading = false
      }
    },

    async createPolicy(data) {
      const policy = await api.createRecordingPolicy(data)
      this.policies.unshift(policy)
      this.policyTotal++
      ElMessage.success('录制策略已创建')
      return policy
    },

    async updatePolicy(id, data) {
      const policy = await api.updateRecordingPolicy(id, data)
      const idx = this.policies.findIndex(p => p.id === id)
      if (idx !== -1) this.policies[idx] = policy
      ElMessage.success('录制策略已更新')
      return policy
    },

    async deletePolicy(id) {
      await api.deleteRecordingPolicy(id)
      this.policies = this.policies.filter(p => p.id !== id)
      this.policyTotal--
      ElMessage.success('录制策略已删除')
    },
  },
})
