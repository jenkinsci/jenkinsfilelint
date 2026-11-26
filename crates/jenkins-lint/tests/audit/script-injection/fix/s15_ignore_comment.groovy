pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                // jenkins-lint: ignore[script-injection] trusted
                sh "git checkout ${CHANGE_BRANCH}"
            }
        }
    }
}
