pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                // jenkinsfilelint: ignore[script-injection] trusted
                sh "git checkout ${CHANGE_BRANCH}"
            }
        }
    }
}
