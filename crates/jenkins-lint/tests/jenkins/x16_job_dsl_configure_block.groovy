job("x") {
    configure { node ->
        def props = node / 'properties'
        props << 'org.jenkinsci.plugins.docker.workflow.declarative.FolderConfig' {
            dockerLabel()
        }
    }
}
