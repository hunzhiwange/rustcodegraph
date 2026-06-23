use super::*;

#[test]
fn connects_controller_resource_bean_interface_impl_end_to_end() {
    let project = TempProject::new("cg-spring-bean");
    let java_dir = "src/main/java/com/example/user";
    project.mkdir("src/main/java/com/example/user/action");
    project.mkdir("src/main/java/com/example/user/bo");
    project.mkdir("src/main/java/com/example/user/service");
    project.mkdir("src/main/java/com/example/user/service/impl");
    project.write(
        "pom.xml",
        "<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency></dependencies></project>\n",
    );
    project.write(
        &format!("{java_dir}/action/UserAction.java"),
        "package com.example.user.action;\n\
         import com.example.user.bo.UserBO;\n\
         import javax.annotation.Resource;\n\
         @org.springframework.stereotype.Controller\n\
         public class UserAction {\n\
           @Resource(name = \"userBO\") private UserBO userbo;\n\
           public void toLogin2() { this.userbo.toLogin2(); }\n\
         }\n",
    );
    project.write(
        &format!("{java_dir}/bo/UserBO.java"),
        "package com.example.user.bo;\n\
         import com.example.user.service.UserService;\n\
         import javax.annotation.Resource;\n\
         @org.springframework.stereotype.Component(\"userBO\")\n\
         public class UserBO {\n\
           @Resource private UserService userService;\n\
           public void toLogin2() { userService.toLogin(); }\n\
         }\n",
    );
    project.write(
        &format!("{java_dir}/service/UserService.java"),
        "package com.example.user.service;\n\
         public interface UserService { void toLogin(); }\n",
    );
    project.write(
        &format!("{java_dir}/service/impl/UserServiceImpl.java"),
        "package com.example.user.service.impl;\n\
         import com.example.user.service.UserService;\n\
         @org.springframework.stereotype.Service(\"userService\")\n\
         public class UserServiceImpl implements UserService {\n\
           public void toLogin() { }\n\
         }\n",
    );

    let mut cg = index(&project);

    let methods = cg.get_nodes_by_kind(NodeKind::Method);
    let find = |class_name: &str, method_name: &str| {
        methods
            .iter()
            .find(|method| {
                method.name == method_name
                    && method.file_path.ends_with(&format!("{class_name}.java"))
            })
            .cloned()
    };

    let action = find("UserAction", "toLogin2").expect("UserAction.toLogin2 should exist");
    let bo = find("UserBO", "toLogin2").expect("UserBO.toLogin2 should exist");
    let svc = find("UserService", "toLogin").expect("UserService.toLogin should exist");
    let impl_method =
        find("UserServiceImpl", "toLogin").expect("UserServiceImpl.toLogin should exist");

    let action_to_bo = cg
        .get_outgoing_edges(&action.id)
        .into_iter()
        .find(|edge| edge.target == bo.id);
    assert!(
        action_to_bo.is_some(),
        "controller `this.userbo.toLogin2()` should reach UserBO.toLogin2"
    );
    assert_eq!(action_to_bo.unwrap().kind, EdgeKind::Calls);

    let bo_to_svc = cg
        .get_outgoing_edges(&bo.id)
        .into_iter()
        .find(|edge| edge.target == svc.id);
    assert!(bo_to_svc.is_some());

    let svc_to_impl = cg
        .get_outgoing_edges(&svc.id)
        .into_iter()
        .find(|edge| edge.target == impl_method.id);
    assert!(svc_to_impl.is_some());

    cg.close();
}

#[test]
fn bridges_a_java_mapper_interface_method_to_its_mybatis_xml_statement_incl_sql_fragments() {
    let project = TempProject::new("cg-mybatis");
    let java_dir = "src/main/java/com/example/dao";
    let xml_dir = "src/main/resources/mappers";
    project.mkdir(java_dir);
    project.mkdir(xml_dir);
    project.write(
        "pom.xml",
        "<project><dependencies><dependency><groupId>org.mybatis</groupId><artifactId>mybatis</artifactId></dependency></dependencies></project>\n",
    );
    project.write(
        &format!("{java_dir}/UserDAOMapper.java"),
        "package com.example.dao;\n\
         public interface UserDAOMapper {\n\
           Object getById(int id);\n\
           int updateUser(Object u);\n\
         }\n",
    );
    project.write(
        &format!("{xml_dir}/UserDAOMapper.xml"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE mapper PUBLIC \"-//mybatis.org//DTD Mapper 3.0//EN\" \"http://mybatis.org/dtd/mybatis-3-mapper.dtd\">\n\
         <mapper namespace=\"com.example.dao.UserDAOMapper\">\n\
           <sql id=\"userCols\">id, name, email</sql>\n\
           <select id=\"getById\" parameterType=\"int\" resultType=\"User\">\n\
             SELECT <include refid=\"userCols\"/> FROM users WHERE id = #{id}\n\
           </select>\n\
           <update id=\"updateUser\" parameterType=\"User\">\n\
             UPDATE users SET name=#{name}, email=#{email} WHERE id=#{id}\n\
           </update>\n\
         </mapper>\n",
    );

    let mut cg = index(&project);

    let methods = cg.get_nodes_by_kind(NodeKind::Method);
    let get_by_id_java = methods
        .iter()
        .find(|method| method.name == "getById" && method.language == Language::Java)
        .expect("Java getById should exist");
    let get_by_id_xml = methods
        .iter()
        .find(|method| method.name == "getById" && method.language == Language::Xml)
        .expect("XML getById should exist");
    let update_java = methods
        .iter()
        .find(|method| method.name == "updateUser" && method.language == Language::Java)
        .expect("Java updateUser should exist");
    let update_xml = methods
        .iter()
        .find(|method| method.name == "updateUser" && method.language == Language::Xml)
        .expect("XML updateUser should exist");
    let sql_frag = methods
        .iter()
        .find(|method| method.name == "userCols" && method.language == Language::Xml)
        .expect("XML userCols SQL fragment should exist");

    assert_eq!(
        get_by_id_xml.qualified_name,
        "com.example.dao.UserDAOMapper::getById"
    );

    let j2x_get = cg
        .get_outgoing_edges(&get_by_id_java.id)
        .into_iter()
        .find(|edge| edge.target == get_by_id_xml.id);
    assert!(
        j2x_get.is_some(),
        "Java getById should reach the XML <select id=\"getById\">"
    );
    assert_eq!(j2x_get.unwrap().kind, EdgeKind::Calls);

    let j2x_upd = cg
        .get_outgoing_edges(&update_java.id)
        .into_iter()
        .find(|edge| edge.target == update_xml.id);
    assert!(
        j2x_upd.is_some(),
        "Java updateUser should reach the XML <update id=\"updateUser\">"
    );

    let inc_edge = cg
        .get_outgoing_edges(&get_by_id_xml.id)
        .into_iter()
        .find(|edge| edge.target == sql_frag.id);
    assert!(
        inc_edge.is_some(),
        "<include refid=\"userCols\"/> should reach the <sql> fragment"
    );

    cg.close();
}

#[test]
fn binds_value_configurationproperties_to_yaml_properties_keys_incl_relaxed_binding() {
    let project = TempProject::new("cg-spring-config");
    let java_dir = "src/main/java/com/example";
    let res_dir = "src/main/resources";
    project.mkdir(java_dir);
    project.mkdir(res_dir);
    project.write(
        "pom.xml",
        "<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter</artifactId></dependency></dependencies></project>\n",
    );
    project.write(
        &format!("{res_dir}/application.yml"),
        "app:\n\
           cache:\n\
             name:\n\
               user-token: \"example-service:auth:token\"\n\
             enabled: true\n\
         db:\n\
           url: \"jdbc:mysql://localhost/x\"\n",
    );
    project.write(
        &format!("{res_dir}/application.properties"),
        "app.retry-count=3\n",
    );
    project.write(
        &format!("{java_dir}/CacheConfig.java"),
        "package com.example;\n\
         import org.springframework.beans.factory.annotation.Value;\n\
         public class CacheConfig {\n\
           @Value(\"${app.cache.name.user-token}\") private String tokenCacheName;\n\
           @Value(\"${app.cache.enabled:true}\") private boolean enabled;\n\
           // relaxed binding: java camelCase, properties kebab-case\n\
           @Value(\"${app.retryCount}\") private int retry;\n\
         }\n",
    );
    project.write(
        &format!("{java_dir}/CacheProperties.java"),
        "package com.example;\n\
         import org.springframework.boot.context.properties.ConfigurationProperties;\n\
         @ConfigurationProperties(prefix = \"app.cache\")\n\
         public class CacheProperties { private boolean enabled; }\n",
    );

    let mut cg = index(&project);

    let cfg_keys = cg
        .get_nodes_by_kind(NodeKind::Constant)
        .into_iter()
        .filter(|node| node.language == Language::Yaml || node.language == Language::Properties)
        .collect::<Vec<_>>();
    let cfg_by_qn = |qn: &str| cfg_keys.iter().find(|node| node.qualified_name == qn);
    assert!(cfg_by_qn("app.cache.name.user-token").is_some());
    assert!(cfg_by_qn("app.cache.enabled").is_some());
    assert!(cfg_by_qn("db.url").is_some());
    assert!(cfg_by_qn("app.retry-count").is_some());

    let value_bindings = cg
        .get_nodes_by_kind(NodeKind::Constant)
        .into_iter()
        .filter(|node| node.id.starts_with("spring-value:"))
        .collect::<Vec<_>>();
    let user_token = value_bindings
        .iter()
        .find(|node| node.name == "app.cache.name.user-token")
        .expect("app.cache.name.user-token binding should exist");
    let user_token_edges = cg.get_outgoing_edges(&user_token.id);
    let user_token_target = user_token_edges.iter().find(|edge| {
        cfg_keys
            .iter()
            .any(|key| key.id == edge.target && key.qualified_name == "app.cache.name.user-token")
    });
    assert!(
        user_token_target.is_some(),
        "@Value should reference the YAML leaf key"
    );

    let enabled_bind = value_bindings
        .iter()
        .find(|node| node.name == "app.cache.enabled")
        .expect("app.cache.enabled binding should exist");
    assert!(cg.get_outgoing_edges(&enabled_bind.id).iter().any(|edge| {
        cfg_by_qn("app.cache.enabled").is_some_and(|target| edge.target == target.id)
    }));

    let retry_bind = value_bindings
        .iter()
        .find(|node| node.name == "app.retryCount")
        .expect("app.retryCount binding should exist");
    assert!(cg.get_outgoing_edges(&retry_bind.id).iter().any(|edge| {
        cfg_by_qn("app.retry-count").is_some_and(|target| edge.target == target.id)
    }));

    let cp_bindings = cg
        .get_nodes_by_kind(NodeKind::Constant)
        .into_iter()
        .filter(|node| node.id.starts_with("spring-cp:"))
        .collect::<Vec<_>>();
    let cp_app_cache = cp_bindings
        .iter()
        .find(|node| node.name == "app.cache")
        .expect("app.cache configuration-properties binding should exist");
    let cp_edges = cg.get_outgoing_edges(&cp_app_cache.id);
    assert!(!cp_edges.is_empty());

    cg.close();
}

#[test]
fn emits_only_a_file_node_for_non_mybatis_xml_pom_xml_beans_xml_log4j_xml() {
    let project = TempProject::new("cg-xml-non-mybatis");
    project.write(
        "pom.xml",
        "<project><groupId>x</groupId><artifactId>y</artifactId></project>\n",
    );
    project.write(
        "log4j.xml",
        "<?xml version=\"1.0\"?><Configuration><Loggers><Root level=\"info\"/></Loggers></Configuration>\n",
    );

    let mut cg = index(&project);
    let xml_method_count = cg
        .get_nodes_by_kind(NodeKind::Method)
        .into_iter()
        .filter(|node| node.language == Language::Xml)
        .count();
    assert_eq!(xml_method_count, 0);
    cg.close();
}

#[test]
fn resolves_a_this_field_method_call_to_a_unique_implementation_class() {
    let project = TempProject::new("cg-java-this-field");
    project.write(
        "App.java",
        "class Svc { public void run() { } }\n\
         class App {\n\
           private Svc svc;\n\
           public void go() { this.svc.run(); }\n\
         }\n",
    );

    let mut cg = index(&project);

    let methods = cg.get_nodes_by_kind(NodeKind::Method);
    let go = methods
        .iter()
        .find(|method| method.name == "go")
        .expect("go should exist");
    let run = methods
        .iter()
        .find(|method| method.name == "run")
        .expect("run should exist");

    let edge = cg
        .get_outgoing_edges(&go.id)
        .into_iter()
        .find(|edge| edge.target == run.id);
    assert!(edge.is_some(), "`this.svc.run()` should resolve to Svc.run");

    cg.close();
}
