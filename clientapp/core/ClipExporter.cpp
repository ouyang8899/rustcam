#include "ClipExporter.h"
#include "common/Logger.h"
#include <QFile>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/error.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>
}

namespace rustcam {

namespace {

QString ffmpegError(int code)
{
    char buf[AV_ERROR_MAX_STRING_SIZE] = {0};
    av_strerror(code, buf, sizeof(buf));
    return QString::fromUtf8(buf);
}

/// 输出统一用毫秒时基：mp4 的 timescale 是 32 位，毫秒足够且安全
const AVRational kOutTimeBase = AVRational{1, 1000};

} // namespace

ClipExporter* ClipExporter::instance()
{
    static ClipExporter exporter;
    return &exporter;
}

ClipExporter::ClipExporter(QObject* parent)
    : QObject(parent)
{
}

ClipExporter::~ClipExporter()
{
}

bool ClipExporter::exportClip(const QString& sourcePath,
                              const QString& outputPath,
                              int64_t startMs,
                              int64_t endMs,
                              QString& error,
                              ProgressCallback progress,
                              int64_t* actualStartMs)
{
    // 优先使用重编码：起点精确到帧。编码器不可用时回退到流复制。
    QString preciseError;
    if (exportClipReencode(sourcePath, outputPath, startMs, endMs, preciseError, progress, actualStartMs)) {
        return true;
    }
    LOG_WARNING("ClipExporter", QString("precise export failed (%1), falling back to stream copy")
        .arg(preciseError));
    if (exportClipStreamCopy(sourcePath, outputPath, startMs, endMs, error, progress, actualStartMs)) {
        return true;
    }
    // 失败时不要留下半成品文件
    QFile::remove(outputPath);
    return false;
}

bool ClipExporter::exportClipReencode(const QString& sourcePath,
                                      const QString& outputPath,
                                      int64_t startMs,
                                      int64_t endMs,
                                      QString& error,
                                      ProgressCallback progress,
                                      int64_t* actualStartMs)
{
    // 捆绑的 FFmpeg 是 LGPL 版本，没有 libx264，用 libopenh264 输出标准 h264
    const AVCodec* encoder = avcodec_find_encoder_by_name("libopenh264");
    if (!encoder) {
        error = tr("没有可用的 h264 编码器");
        return false;
    }

    AVFormatContext* in = nullptr;
    int ret = avformat_open_input(&in, sourcePath.toUtf8().constData(), nullptr, nullptr);
    if (ret < 0) {
        error = tr("无法打开源文件：%1").arg(ffmpegError(ret));
        return false;
    }
    if (avformat_find_stream_info(in, nullptr) < 0) {
        avformat_close_input(&in);
        error = tr("无法读取源文件信息");
        return false;
    }

    int videoStreamIndex = -1;
    for (unsigned int i = 0; i < in->nb_streams; ++i) {
        if (in->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            videoStreamIndex = static_cast<int>(i);
            break;
        }
    }
    if (videoStreamIndex < 0) {
        avformat_close_input(&in);
        error = tr("源文件没有视频流");
        return false;
    }

    AVFormatContext* out = nullptr;
    ret = avformat_alloc_output_context2(&out, nullptr, nullptr, outputPath.toUtf8().constData());
    if (ret < 0 || !out) {
        avformat_close_input(&in);
        error = tr("无法创建输出文件：%1").arg(ffmpegError(ret));
        return false;
    }

    // 视频流：重编码
    const AVCodecParameters* inPar = in->streams[videoStreamIndex]->codecpar;
    const AVCodec* decoder = avcodec_find_decoder(inPar->codec_id);
    AVCodecContext* decCtx = decoder ? avcodec_alloc_context3(decoder) : nullptr;
    if (!decCtx
        || avcodec_parameters_to_context(decCtx, inPar) < 0
        || avcodec_open2(decCtx, decoder, nullptr) < 0) {
        if (decCtx) {
            avcodec_free_context(&decCtx);
        }
        avformat_close_input(&in);
        avformat_free_context(out);
        error = tr("无法打开视频解码器");
        return false;
    }

    AVCodecContext* encCtx = avcodec_alloc_context3(encoder);
    if (!encCtx) {
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        avformat_free_context(out);
        error = tr("无法创建编码器");
        return false;
    }

    encCtx->width = decCtx->width;
    encCtx->height = decCtx->height;
    encCtx->pix_fmt = AV_PIX_FMT_YUV420P;
    encCtx->time_base = AVRational{1, 1000};
    encCtx->framerate = av_guess_frame_rate(in, in->streams[videoStreamIndex], nullptr);
    if (encCtx->framerate.num <= 0 || encCtx->framerate.den <= 0) {
        encCtx->framerate = AVRational{25, 1};
    }
    encCtx->max_b_frames = 0;   // 简化 B 帧带来的 pts/dts 重排
    encCtx->gop_size = 30;
    encCtx->bit_rate = inPar->bit_rate > 0 ? inPar->bit_rate : 2000000;

    if (avcodec_open2(encCtx, encoder, nullptr) < 0) {
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        avformat_free_context(out);
        error = tr("无法打开编码器");;
        return false;
    }

    AVStream* outVideo = avformat_new_stream(out, nullptr);
    if (!outVideo
        || avcodec_parameters_from_context(outVideo->codecpar, encCtx) < 0) {
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        avformat_free_context(out);
        error = tr("无法创建输出视频流");;
        return false;
    }
    outVideo->time_base = encCtx->time_base;
    if (encCtx->extradata_size > 0) {
        outVideo->codecpar->extradata = static_cast<uint8_t*>(
            av_mallocz(encCtx->extradata_size + AV_INPUT_BUFFER_PADDING_SIZE));
        memcpy(outVideo->codecpar->extradata, encCtx->extradata, encCtx->extradata_size);
        outVideo->codecpar->extradata_size = encCtx->extradata_size;
    }

    // 音频流：流复制（LGPL 没有 aac 编码器）
    int audioStreamIndex = -1;
    for (unsigned int i = 0; i < in->nb_streams; ++i) {
        if (i == static_cast<unsigned int>(videoStreamIndex)) {
            continue;
        }
        if (in->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            audioStreamIndex = static_cast<int>(i);
            break;
        }
    }
    AVStream* outAudio = nullptr;
    if (audioStreamIndex >= 0) {
        outAudio = avformat_new_stream(out, nullptr);
        if (!outAudio
            || avcodec_parameters_copy(outAudio->codecpar, in->streams[audioStreamIndex]->codecpar) < 0) {
            avcodec_free_context(&encCtx);
            avcodec_free_context(&decCtx);
            avformat_close_input(&in);
            avformat_free_context(out);
            error = tr("无法创建输出音频流");;
            return false;
        }
        outAudio->codecpar->codec_tag = 0;
        outAudio->time_base = kOutTimeBase;
    }

    if (!(out->oformat->flags & AVFMT_NOFILE)) {
        ret = avio_open(&out->pb, outputPath.toUtf8().constData(), AVIO_FLAG_WRITE);
        if (ret < 0) {
            avcodec_free_context(&encCtx);
            avcodec_free_context(&decCtx);
            avformat_close_input(&in);
            avformat_free_context(out);
            error = tr("无法写入输出文件：%1").arg(ffmpegError(ret));
            return false;
        }
    }

    ret = avformat_write_header(out, nullptr);
    if (ret < 0) {
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        if (!(out->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&out->pb);
        }
        avformat_free_context(out);
        error = tr("写文件头失败：%1").arg(ffmpegError(ret));
        return false;
    }

    // 定位到起点附近
    {
        const AVRational tb = in->streams[videoStreamIndex]->time_base;
        const int64_t target = av_rescale_q(startMs, kOutTimeBase, tb);
        av_seek_frame(in, videoStreamIndex, target, AVSEEK_FLAG_BACKWARD);
    }

    SwsContext* sws = sws_getContext(decCtx->width, decCtx->height, (AVPixelFormat)decCtx->pix_fmt,
                                      encCtx->width, encCtx->height, encCtx->pix_fmt,
                                      SWS_BILINEAR, nullptr, nullptr, nullptr);
    if (!sws) {
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        if (!(out->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&out->pb);
        }
        avformat_free_context(out);
        error = tr("无法创建像素格式转换器");;
        return false;
    }

    AVPacket* pkt = av_packet_alloc();
    AVPacket* outPkt = av_packet_alloc();
    AVFrame* frame = av_frame_alloc();
    AVFrame* encFrame = av_frame_alloc();
    encFrame->format = encCtx->pix_fmt;
    encFrame->width = encCtx->width;
    encFrame->height = encCtx->height;
    if (av_frame_get_buffer(encFrame, 0) < 0) {
        sws_freeContext(sws);
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        if (!(out->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&out->pb);
        }
        avformat_free_context(out);
        error = tr("无法分配编码缓冲");;
        return false;
    }
    bool wrote = false;
    int64_t frameIndex = 0;
    int lastPercent = -1;
    if (actualStartMs) {
        *actualStartMs = startMs;   // 重编码起点精确
    }

    auto reportProgress = [&](int64_t positionMs) {
        if (!progress || endMs <= startMs) {
            return;
        }
        const int percent = static_cast<int>((positionMs - startMs) * 100 / (endMs - startMs));
        if (percent <= lastPercent) {
            return;
        }
        lastPercent = percent;
        progress(percent, tr("正在裁剪..."));
    };
    reportProgress(startMs);

    auto cleanup = [&]() {
        sws_freeContext(sws);
        av_packet_free(&pkt);
        av_packet_free(&outPkt);
        av_frame_free(&frame);
        av_frame_free(&encFrame);
        avcodec_free_context(&encCtx);
        avcodec_free_context(&decCtx);
        avformat_close_input(&in);
        if (out) {
            if (!(out->oformat->flags & AVFMT_NOFILE)) {
                avio_closep(&out->pb);
            }
            avformat_free_context(out);
        }
    };

    while (av_read_frame(in, pkt) >= 0) {
        const int streamIndex = pkt->stream_index;

        // 音频：按区间过滤后直接复制
        if (audioStreamIndex >= 0 && streamIndex == audioStreamIndex && outAudio) {
            const AVRational tb = in->streams[streamIndex]->time_base;
            const int64_t ptsMs = av_rescale_q(pkt->pts, tb, kOutTimeBase);
            if (ptsMs >= startMs && ptsMs <= endMs) {
                pkt->pts = ptsMs - startMs;
                pkt->dts = av_rescale_q(pkt->dts, tb, kOutTimeBase) - startMs;
                av_interleaved_write_frame(out, pkt);
            }
            av_packet_unref(pkt);
            continue;
        }

        if (streamIndex != videoStreamIndex) {
            av_packet_unref(pkt);
            continue;
        }

        if (avcodec_send_packet(decCtx, pkt) == 0) {
            while (avcodec_receive_frame(decCtx, frame) == 0) {
                const AVRational tb = in->streams[videoStreamIndex]->time_base;
                const int64_t frameMs = frame->pts >= 0
                    ? av_rescale_q(frame->pts, tb, kOutTimeBase)
                    : frameIndex * 1000 / (encCtx->framerate.num > 0 ? encCtx->framerate.num : 25);

                if (frameMs < startMs) {
                    av_frame_unref(frame);
                    continue;
                }
                if (frameMs > endMs) {
                    av_frame_unref(frame);
                    goto done;
                }

                if (av_frame_make_writable(encFrame) < 0
                    || av_frame_copy_props(encFrame, frame) < 0) {
                    av_frame_unref(frame);
                    break;
                }
                encFrame->format = encCtx->pix_fmt;
                // 统一转成编码器要求的像素格式
                sws_scale(sws, frame->data, frame->linesize, 0, decCtx->height,
                          encFrame->data, encFrame->linesize);
                encFrame->pts = frameMs - startMs;
                reportProgress(frameMs);

                if (avcodec_send_frame(encCtx, encFrame) == 0) {
                    while (avcodec_receive_packet(encCtx, outPkt) == 0) {
                        av_packet_rescale_ts(outPkt, encCtx->time_base, outVideo->time_base);
                        av_interleaved_write_frame(out, outPkt);
                        wrote = true;
                        av_packet_unref(outPkt);
                    }
                }
                ++frameIndex;
                av_frame_unref(frame);
            }
        }
        av_packet_unref(pkt);
    }

done:
    // 冲刷编码器
    avcodec_send_frame(encCtx, nullptr);
    {
        while (avcodec_receive_packet(encCtx, outPkt) == 0) {
            av_packet_rescale_ts(outPkt, encCtx->time_base, outVideo->time_base);
            av_interleaved_write_frame(out, outPkt);
            wrote = true;
            av_packet_unref(outPkt);
        }
    }

    av_write_trailer(out);
    cleanup();

    if (!wrote) {
        error = tr("导出区间内没有可用的数据");
        LOG_ERROR("ClipExporter", error);
        QFile::remove(outputPath);
        return false;
    }

    LOG_INFO("ClipExporter", QString("clip exported (re-encoded): %1 -> %2 (%3ms~%4ms, frames=%5)")
        .arg(sourcePath).arg(outputPath).arg(startMs).arg(endMs).arg(frameIndex));
    return true;
}

bool ClipExporter::exportClipStreamCopy(const QString& sourcePath,
                                        const QString& outputPath,
                                        int64_t startMs,
                                        int64_t endMs,
                                        QString& error,
                                        ProgressCallback progress,
                                        int64_t* actualStartMs)
{
    if (sourcePath.isEmpty() || outputPath.isEmpty()) {
        error = tr("文件路径无效");
        return false;
    }
    if (endMs <= startMs) {
        error = tr("结束时间必须晚于开始时间");
        return false;
    }

    AVFormatContext* in = nullptr;
    int ret = avformat_open_input(&in, sourcePath.toUtf8().constData(), nullptr, nullptr);
    if (ret < 0) {
        error = tr("无法打开源文件：%1").arg(ffmpegError(ret));
        LOG_ERROR("ClipExporter", error);
        return false;
    }

    if (avformat_find_stream_info(in, nullptr) < 0) {
        avformat_close_input(&in);
        error = tr("无法读取源文件信息");
        return false;
    }

    AVFormatContext* out = nullptr;
    ret = avformat_alloc_output_context2(&out, nullptr, nullptr, outputPath.toUtf8().constData());
    if (ret < 0 || !out) {
        avformat_close_input(&in);
        error = tr("无法创建输出文件：%1").arg(ffmpegError(ret));
        LOG_ERROR("ClipExporter", error);
        return false;
    }

    // 复制所有流（视频 + 音频）
    for (unsigned int i = 0; i < in->nb_streams; ++i) {
        AVStream* outStream = avformat_new_stream(out, nullptr);
        if (!outStream) {
            avformat_close_input(&in);
            avio_closep(&out->pb);
            avformat_free_context(out);
            error = tr("无法创建输出流");
            return false;
        }
        if (avcodec_parameters_copy(outStream->codecpar, in->streams[i]->codecpar) < 0) {
            avformat_close_input(&in);
            avio_closep(&out->pb);
            avformat_free_context(out);
            error = tr("无法复制流参数");
            return false;
        }
        outStream->codecpar->codec_tag = 0;
        outStream->time_base = kOutTimeBase;
    }

    if (!(out->oformat->flags & AVFMT_NOFILE)) {
        ret = avio_open(&out->pb, outputPath.toUtf8().constData(), AVIO_FLAG_WRITE);
        if (ret < 0) {
            avformat_close_input(&in);
            avformat_free_context(out);
            error = tr("无法写入输出文件：%1").arg(ffmpegError(ret));
            return false;
        }
    }

    ret = avformat_write_header(out, nullptr);
    if (ret < 0) {
        avformat_close_input(&in);
        if (!(out->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&out->pb);
        }
        avformat_free_context(out);
        error = tr("写文件头失败：%1").arg(ffmpegError(ret));
        return false;
    }

    // 定位到起点（取最近的关键帧）
    bool hasVideo = false;
    int videoStream = -1;
    for (unsigned int i = 0; i < in->nb_streams; ++i) {
        if (in->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            hasVideo = true;
            videoStream = static_cast<int>(i);
            break;
        }
    }

    if (hasVideo) {
        const AVRational tb = in->streams[videoStream]->time_base;
        const int64_t target = av_rescale_q(startMs, kOutTimeBase, tb);
        av_seek_frame(in, videoStream, target, AVSEEK_FLAG_BACKWARD);
    }

    AVPacket* pkt = av_packet_alloc();
    bool wrote = false;
    int lastPercent = -1;
    int64_t writtenStartMs = -1;
    auto reportProgress = [&](int64_t positionMs) {
        if (!progress || endMs <= startMs) {
            return;
        }
        const int percent = static_cast<int>((positionMs - startMs) * 100 / (endMs - startMs));
        if (percent <= lastPercent) {
            return;
        }
        lastPercent = percent;
        progress(percent, tr("正在裁剪..."));
    };
    while (av_read_frame(in, pkt) >= 0) {
        const int streamIndex = pkt->stream_index;
        if (streamIndex < 0 || static_cast<unsigned int>(streamIndex) >= in->nb_streams) {
            av_packet_unref(pkt);
            continue;
        }
        const AVRational tb = in->streams[streamIndex]->time_base;
        const int64_t ptsMs = av_rescale_q(pkt->pts, tb, kOutTimeBase);
        const int64_t dtsMs = av_rescale_q(pkt->dts, tb, kOutTimeBase);

        if (ptsMs < startMs) {
            av_packet_unref(pkt);
            continue;
        }
        if (ptsMs > endMs) {
            av_packet_unref(pkt);
            break;
        }

        if (writtenStartMs < 0) {
            // 流复制的第一个包通常落在关键帧上，实际起点可能晚于请求起点
            writtenStartMs = ptsMs;
            if (actualStartMs) {
                *actualStartMs = ptsMs;
            }
        }
        reportProgress(ptsMs);

        pkt->pts = ptsMs - startMs;
        pkt->dts = dtsMs - startMs;
        if (pkt->duration > 0) {
            pkt->duration = av_rescale_q(pkt->duration, tb, kOutTimeBase);
        }

        ret = av_interleaved_write_frame(out, pkt);
        av_packet_unref(pkt);
        if (ret < 0) {
            break;
        }
        wrote = true;
    }

    av_packet_free(&pkt);
    av_write_trailer(out);
    if (!(out->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out->pb);
    }
    avformat_free_context(out);
    avformat_close_input(&in);

    if (!wrote) {
        error = tr("导出区间内没有可用的数据");
        LOG_ERROR("ClipExporter", error);
        QFile::remove(outputPath);
        return false;
    }

    LOG_INFO("ClipExporter", QString("clip exported: %1 -> %2 (%3ms~%4ms)")
        .arg(sourcePath).arg(outputPath).arg(startMs).arg(endMs));
    return true;
}

}
