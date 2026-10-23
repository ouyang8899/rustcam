import { request } from '../utils/request'

export async function getRecordings(params = {}) {
  const res = await request.get('/recordings', { params })
  return res.data.data
}

export async function getRecording(id) {
  const res = await request.get(`/recordings/${id}`)
  return res.data.data
}

export async function getRecordingStats() {
  const res = await request.get('/recordings/stats')
  return res.data.data
}

export async function getRecordingFilesById(id) {
  const res = await request.get(`/recordings/${id}/files`)
  return res.data.data || []
}

export async function createRecording(data) {
  const res = await request.post('/recordings', data)
  return res.data.data
}

export async function startRecording(id) {
  const res = await request.post(`/recordings/${id}/start`)
  return res.data.data
}

export async function stopRecording(id) {
  const res = await request.post(`/recordings/${id}/stop`)
  return res.data.data
}

export async function pauseRecording(id) {
  const res = await request.post(`/recordings/${id}/pause`)
  return res.data.data
}

export async function resumeRecording(id) {
  const res = await request.post(`/recordings/${id}/resume`)
  return res.data.data
}

export async function deleteRecording(id) {
  const res = await request.delete(`/recordings/${id}`)
  return res.data
}

export async function getAllRecordingFiles() {
  const res = await request.get('/recordings/files')
  return res.data.data || []
}

export async function getRecordingFilesWithParams(params) {
  const res = await request.get('/recordings/files', { params })
  return res.data.data
}

export async function getRecordingFilePlayUrl(fileId) {
  const res = await request.get(`/recordings/files/${fileId}/play-url`)
  return res.data.data
}

export async function getRecordingPolicies(params) {
  const res = await request.get('/recording-policies', { params })
  return res.data.data
}

export async function createRecordingPolicy(data) {
  const res = await request.post('/recording-policies', data)
  return res.data.data
}

export async function updateRecordingPolicy(id, data) {
  const res = await request.put(`/recording-policies/${id}`, data)
  return res.data.data
}

export async function deleteRecordingPolicy(id) {
  const res = await request.delete(`/recording-policies/${id}`)
  return res.data
}
