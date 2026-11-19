// Every node these accept is the controller.
node('built-in && linux') {
    sh 'make'
}

// The controller is one of the nodes these accept.
node('linux || master') {
    sh 'make'
}

pipeline {
    agent { label '(built-in || docker) && amd64' }
    stages {
        stage('Build') {
            steps {
                sh 'make'
            }
        }
    }
}
