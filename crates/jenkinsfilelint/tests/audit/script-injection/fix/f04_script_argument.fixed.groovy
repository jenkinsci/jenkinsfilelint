pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def out = sh(script: 'git log "$CHANGE_BRANCH"', returnStdout: true)
                }
            }
        }
    }
}
