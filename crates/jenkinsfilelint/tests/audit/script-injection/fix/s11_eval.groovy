pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh "eval ${CHANGE_TITLE}"
            }
        }
    }
}
