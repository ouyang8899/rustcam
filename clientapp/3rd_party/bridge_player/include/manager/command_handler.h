#pragma once
#include"manager/command_channel.h"
namespace BridgePlayer{
class CommandHandler{
    public:
    CommandHandler() = default;
    virtual ~CommandHandler() = default;
  public:
    virtual void run(CommandChannel::Ptr channel,
    const std::string& type,
    const std::string& requestId,
    const std::string& cRequestId,
    const Json::Value& data) = 0;
};

class ClientCommandHandler: public CommandHandler{
    using Processor = std::function<void(CommandChannel::Ptr channel,const std::string& requestId, const std::string& cRequestId,const Json::Value& data)>;
    public:
    ClientCommandHandler();
    public:
    void run(CommandChannel::Ptr channel,
    const std::string& type,
    const std::string& requestId,
    const std::string& cRequestId,
    const Json::Value& data) override;
    protected:
    void init_processors();
    protected:
    std::map<std::string,Processor> processor_map_;
};
}