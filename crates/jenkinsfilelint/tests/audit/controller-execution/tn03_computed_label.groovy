// A label that Groovy computes is not known to the audit.
def where = params.AGENT ?: 'master'

node(where) {
    sh 'make'
}

node("${env.NODE_LABEL}") {
    sh 'make'
}

pipeline {
    agent { label params.AGENT }
    stages {
        stage('Build') {
            steps {
                sh 'make'
            }
        }
    }
}
