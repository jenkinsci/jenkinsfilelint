pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def dir = "src"
                    sh "cd ${dir} && git checkout ${CHANGE_BRANCH}"
                }
            }
        }
    }
}
