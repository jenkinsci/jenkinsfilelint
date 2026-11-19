pipeline {
    agent {
        node {
            label 'built-in'
            customWorkspace '/var/lib/jenkins/shared'
        }
    }
    stages {
        stage('Build') {
            steps {
                sh 'make'
            }
        }
    }
}
