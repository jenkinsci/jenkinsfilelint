podTemplate(containers: [
    containerTemplate(name: 'golang', image: 'golang', command: 'sleep', args: '99d'),
    containerTemplate(name: 'kaniko', image: 'gcr.io/kaniko-project/executor:latest', command: 'sleep', args: '99d'),
]) {
    node(POD_LABEL) {
        container('golang') {
            sh 'go test ./...'
        }
    }
}
