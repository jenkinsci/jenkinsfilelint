pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh """
                    git fetch origin
                    git checkout ${CHANGE_BRANCH}
                """
            }
        }
    }
}
