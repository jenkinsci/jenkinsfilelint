pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "git checkout ${CHANGE_BRANCH} ${RELEASE}"
            }
        }
    }
}
