pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                sh """
                    cat <<EOF
                    ${CHANGE_TITLE}
                    EOF
                """
            }
        }
    }
}
