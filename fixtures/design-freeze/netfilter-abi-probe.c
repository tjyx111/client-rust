/* Future driver-ABI design fixture; not part of the S0 environment gate. */
#define _C_API
#define WIN32

#include <windows.h>
#include <stddef.h>

#include "nfapi.h"

#define ABI_ASSERT(name, expression) typedef char abi_assert_##name[(expression) ? 1 : -1]

ABI_ASSERT(endpoint_id_size, sizeof(ENDPOINT_ID) == 8);
ABI_ASSERT(endpoint_id_align, __alignof(ENDPOINT_ID) == 8);

ABI_ASSERT(rule_size, sizeof(NF_RULE) == 83);
ABI_ASSERT(rule_align, __alignof(NF_RULE) == 1);
ABI_ASSERT(rule_protocol, offsetof(NF_RULE, protocol) == 0);
ABI_ASSERT(rule_process_id, offsetof(NF_RULE, processId) == 4);
ABI_ASSERT(rule_direction, offsetof(NF_RULE, direction) == 8);
ABI_ASSERT(rule_local_port, offsetof(NF_RULE, localPort) == 9);
ABI_ASSERT(rule_remote_port, offsetof(NF_RULE, remotePort) == 11);
ABI_ASSERT(rule_ip_family, offsetof(NF_RULE, ip_family) == 13);
ABI_ASSERT(rule_local_ip, offsetof(NF_RULE, localIpAddress) == 15);
ABI_ASSERT(rule_local_mask, offsetof(NF_RULE, localIpAddressMask) == 31);
ABI_ASSERT(rule_remote_ip, offsetof(NF_RULE, remoteIpAddress) == 47);
ABI_ASSERT(rule_remote_mask, offsetof(NF_RULE, remoteIpAddressMask) == 63);
ABI_ASSERT(rule_filtering_flag, offsetof(NF_RULE, filteringFlag) == 79);

ABI_ASSERT(tcp_info_size, sizeof(NF_TCP_CONN_INFO) == 67);
ABI_ASSERT(tcp_info_align, __alignof(NF_TCP_CONN_INFO) == 1);
ABI_ASSERT(tcp_info_filtering_flag, offsetof(NF_TCP_CONN_INFO, filteringFlag) == 0);
ABI_ASSERT(tcp_info_process_id, offsetof(NF_TCP_CONN_INFO, processId) == 4);
ABI_ASSERT(tcp_info_direction, offsetof(NF_TCP_CONN_INFO, direction) == 8);
ABI_ASSERT(tcp_info_ip_family, offsetof(NF_TCP_CONN_INFO, ip_family) == 9);
ABI_ASSERT(tcp_info_local, offsetof(NF_TCP_CONN_INFO, localAddress) == 11);
ABI_ASSERT(tcp_info_remote, offsetof(NF_TCP_CONN_INFO, remoteAddress) == 39);

ABI_ASSERT(udp_info_size, sizeof(NF_UDP_CONN_INFO) == 34);
ABI_ASSERT(udp_info_align, __alignof(NF_UDP_CONN_INFO) == 1);
ABI_ASSERT(udp_info_process_id, offsetof(NF_UDP_CONN_INFO, processId) == 0);
ABI_ASSERT(udp_info_ip_family, offsetof(NF_UDP_CONN_INFO, ip_family) == 4);
ABI_ASSERT(udp_info_local, offsetof(NF_UDP_CONN_INFO, localAddress) == 6);

ABI_ASSERT(udp_request_size, sizeof(NF_UDP_CONN_REQUEST) == 66);
ABI_ASSERT(udp_request_align, __alignof(NF_UDP_CONN_REQUEST) == 1);
ABI_ASSERT(udp_request_filtering_flag, offsetof(NF_UDP_CONN_REQUEST, filteringFlag) == 0);
ABI_ASSERT(udp_request_process_id, offsetof(NF_UDP_CONN_REQUEST, processId) == 4);
ABI_ASSERT(udp_request_ip_family, offsetof(NF_UDP_CONN_REQUEST, ip_family) == 8);
ABI_ASSERT(udp_request_local, offsetof(NF_UDP_CONN_REQUEST, localAddress) == 10);
ABI_ASSERT(udp_request_remote, offsetof(NF_UDP_CONN_REQUEST, remoteAddress) == 38);

ABI_ASSERT(udp_options_size, sizeof(NF_UDP_OPTIONS) == 9);
ABI_ASSERT(udp_options_align, __alignof(NF_UDP_OPTIONS) == 1);
ABI_ASSERT(udp_options_flags, offsetof(NF_UDP_OPTIONS, flags) == 0);
ABI_ASSERT(udp_options_length, offsetof(NF_UDP_OPTIONS, optionsLength) == 4);
ABI_ASSERT(udp_options_data, offsetof(NF_UDP_OPTIONS, options) == 8);

ABI_ASSERT(ip_options_size, sizeof(NF_IP_PACKET_OPTIONS) == 22);
ABI_ASSERT(ip_options_align, __alignof(NF_IP_PACKET_OPTIONS) == 1);
ABI_ASSERT(ip_options_family, offsetof(NF_IP_PACKET_OPTIONS, ip_family) == 0);
ABI_ASSERT(ip_options_header_size, offsetof(NF_IP_PACKET_OPTIONS, ipHeaderSize) == 2);
ABI_ASSERT(ip_options_compartment, offsetof(NF_IP_PACKET_OPTIONS, compartmentId) == 6);
ABI_ASSERT(ip_options_interface, offsetof(NF_IP_PACKET_OPTIONS, interfaceIndex) == 10);
ABI_ASSERT(ip_options_subinterface, offsetof(NF_IP_PACKET_OPTIONS, subInterfaceIndex) == 14);
ABI_ASSERT(ip_options_flags, offsetof(NF_IP_PACKET_OPTIONS, flags) == 18);

ABI_ASSERT(event_handler_size, sizeof(NF_EventHandler) == 128);
ABI_ASSERT(event_handler_align, __alignof(NF_EventHandler) == 1);
ABI_ASSERT(ip_event_handler_size, sizeof(NF_IPEventHandler) == 16);
ABI_ASSERT(ip_event_handler_align, __alignof(NF_IPEventHandler) == 1);

ABI_ASSERT(max_address_length, NF_MAX_ADDRESS_LENGTH == 28);
ABI_ASSERT(direction_in, NF_D_IN == 1);
ABI_ASSERT(direction_out, NF_D_OUT == 2);
ABI_ASSERT(allow_flag, NF_ALLOW == 0);
ABI_ASSERT(filter_flag, NF_FILTER == 2);
ABI_ASSERT(connect_request_flag, NF_INDICATE_CONNECT_REQUESTS == 16);
ABI_ASSERT(ip_packet_flag, NF_FILTER_AS_IP_PACKETS == 128);
ABI_ASSERT(success_status, NF_STATUS_SUCCESS == 0);

typedef NF_STATUS (__cdecl *expected_nf_init)(const char *, NF_EventHandler *);
typedef NF_STATUS (__cdecl *expected_nf_udp_post_receive)(
    ENDPOINT_ID, const unsigned char *, const char *, int, PNF_UDP_OPTIONS);

static void check_function_signatures(void)
{
    expected_nf_init const init_function = &nf_init;
    expected_nf_udp_post_receive const post_receive_function = &nf_udpPostReceive;
    (void)init_function;
    (void)post_receive_function;
}
