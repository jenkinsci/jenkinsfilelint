pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    sh(script: 'ls',
                        returnStdout: true)
                }
            }
        }
    }
}
