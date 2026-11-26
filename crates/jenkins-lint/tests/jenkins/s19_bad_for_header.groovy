pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    for (int i = 0; i < ; i++) { echo "${i}" }
                }
            }
        }
    }
}
