// EmberWatch original persistent transport adapter. MIT, no third-party code copied.
// stdin: P\t<topic>\t<type>\t<textproto> or S\t<service>\t<reqtype>\t<reptype>\t<textproto>
#include <gz/transport/Node.hh>
#include <gz/msgs/Factory.hh>
#include <gz/msgs/marker.pb.h>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

int main() {
  gz::transport::Node node;
  std::map<std::string, gz::transport::Node::Publisher> pubs;
  std::string line;
  while (std::getline(std::cin, line)) {
    std::vector<std::string> part;
    std::stringstream stream(line);
    std::string item;
    while (std::getline(stream, item, '\t')) part.push_back(item);
    if (part.size() < 4) continue;
    const auto &kind = part[0], &topic = part[1], &type = part[2];
    auto msg = gz::msgs::Factory::New(type, part.back());
    if (!msg) { std::cerr << "invalid message " << topic << std::endl; continue; }
    if (kind == "P") {
      if (!pubs.count(topic)) pubs.emplace(topic, node.Advertise(topic, type));
      bool ok = pubs.at(topic).Publish(*msg);
      std::cout << "publish " << topic << " " << ok << std::endl;
    } else if (kind == "S" && part.size() == 5) {
      // Gazebo's marker endpoint is a request-only service. Empty is its advertised
      // response TYPE, but there is no response callback / acknowledgement to await.
      if (part[3] == "gz.msgs.Empty") {
        auto marker = dynamic_cast<const gz::msgs::Marker *>(msg.get());
        bool ok = marker && node.Request(topic, *marker);
        std::cout << "oneway " << topic << " requested=" << ok << std::endl;
        continue;
      }
      std::string response;
      bool result = false;
      bool ok = node.RequestRaw(topic, msg->SerializeAsString(), type, part[3], 750, response, result);
      std::cout << "service " << topic << " executed=" << ok << " result=" << result << std::endl;
    }
  }
  return 0;
}
