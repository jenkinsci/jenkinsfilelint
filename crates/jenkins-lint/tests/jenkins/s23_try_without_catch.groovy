pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    try {
                        sh 'make'
                    }
                }
            }
        }
    }
}
