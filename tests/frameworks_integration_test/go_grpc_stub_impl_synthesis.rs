use super::*;

#[test]
fn bridges_unimplemented_msg_server_methods_to_the_hand_written_keeper_impl() {
    let project = TempProject::new("cg-go-grpc");
    project.write(
        "tx_grpc.pb.go",
        "package banktypes\n\n\
         type UnimplementedMsgServer struct{}\n\n\
         func (UnimplementedMsgServer) Send(ctx context.Context, req *MsgSend) (*MsgSendResponse, error) { return nil, nil }\n\
         func (UnimplementedMsgServer) MultiSend(ctx context.Context, req *MsgMultiSend) (*MsgMultiSendResponse, error) { return nil, nil }\n\
         func (UnimplementedMsgServer) mustEmbedUnimplementedMsgServer() {}\n\
         func (UnimplementedMsgServer) testEmbeddedByValue() {}\n",
    );
    project.write(
        "msg_server.go",
        "package keeper\n\n\
         type msgServer struct{ k Keeper }\n\n\
         func (m msgServer) Send(ctx context.Context, req *MsgSend) (*MsgSendResponse, error) {\n\
           return m.k.SendCoins(ctx, req.From, req.To, req.Amount)\n\
         }\n\
         func (m msgServer) MultiSend(ctx context.Context, req *MsgMultiSend) (*MsgMultiSendResponse, error) {\n\
           return nil, nil\n\
         }\n",
    );

    let mut cg = index(&project);

    let stub_send = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| {
            node.qualified_name
                .ends_with("UnimplementedMsgServer::Send")
        })
        .expect("UnimplementedMsgServer.Send should be indexed");
    let impl_send = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .find(|node| node.qualified_name.ends_with("msgServer::Send"))
        .expect("msgServer.Send should be indexed");

    let bridge = cg
        .get_outgoing_edges(&stub_send.id)
        .into_iter()
        .find(|edge| edge.target == impl_send.id && edge.kind == EdgeKind::Calls)
        .expect("stub Send should bridge to impl Send");
    assert_eq!(bridge.provenance, Some(EdgeProvenance::Heuristic));
    assert_eq!(
        edge_metadata_str(&bridge, "synthesizedBy"),
        Some("go-grpc-stub-impl")
    );

    cg.close();
}

#[test]
fn does_not_bridge_to_candidates_living_in_another_generated_file() {
    let project = TempProject::new("cg-go-grpc-sib");
    project.write(
        "tx_grpc.pb.go",
        "package banktypes\n\n\
         type UnimplementedMsgServer struct{}\n\
         func (UnimplementedMsgServer) Send() {}\n\
         func (UnimplementedMsgServer) MultiSend() {}\n\n\
         type msgClient struct{}\n\
         func (m msgClient) Send() {}\n\
         func (m msgClient) MultiSend() {}\n",
    );

    let mut cg = index(&project);

    let stub = cg
        .get_nodes_by_kind(NodeKind::Struct)
        .into_iter()
        .find(|node| node.name == "UnimplementedMsgServer");
    assert!(stub.is_some());
    let bridges = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .filter(|node| {
            node.qualified_name
                .ends_with("UnimplementedMsgServer::Send")
        })
        .flat_map(|stub_send| cg.get_outgoing_edges(&stub_send.id))
        .filter(|edge| {
            edge.kind == EdgeKind::Calls
                && edge_metadata_str(edge, "synthesizedBy") == Some("go-grpc-stub-impl")
        })
        .collect::<Vec<_>>();
    assert!(
        bridges.is_empty(),
        "no bridge to msgClient (also generated)"
    );

    cg.close();
}
